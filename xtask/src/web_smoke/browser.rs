//! Chrome/Chromium driver for browser-smoke checkpoints (#144/#168).
//!
//! ## Driver choice
//!
//! Drives a system-installed Chrome/Chromium directly over the Chrome
//! DevTools Protocol (CDP), via the `headless_chrome` crate -- not a
//! WebDriver client (e.g. `fantoccini`) against a separately installed
//! `chromedriver`. This machine has Google Chrome installed but no
//! `chromedriver` on `PATH`, and pinning a `chromedriver` *version* to match
//! whatever Chrome build a given dev machine or CI runner happens to have is
//! its own ongoing maintenance burden. `headless_chrome` launches the
//! browser's own binary and speaks CDP directly, so the only external
//! dependency is "a Chrome/Chromium binary exists somewhere findable" --
//! [`chrome_binary`] wraps `headless_chrome::browser::default_executable`,
//! which checks a `CHROME` env var override, then `google-chrome-stable`/
//! `chromium`/etc. on `PATH`, then the standard macOS `/Applications`
//! install locations. No version pinning or matrix to maintain.
//!
//! ## Determinism
//!
//! A screenshot must compare sensibly against a baseline captured on a
//! *different* machine (a dev laptop today, a GitHub Actions runner
//! tomorrow), so every checkpoint forces software rendering
//! (`--use-angle=swiftshader`; `--enable-unsafe-swiftshader` because Chrome
//! 129+ requires it to allow software WebGL in headless mode at all)
//! instead of trusting whatever GPU driver happens to be on the host, and
//! forces `--force-device-scale-factor={dpr}` at the Chrome-launch level
//! (#278; see [`launch`]'s doc comment) so a Retina/HiDPI dev machine and a
//! DPR-1 CI runner both produce the exact same, checkpoint-requested backing
//! resolution regardless of the host's own display -- still fully
//! host-independent, just no longer pinned to a hardcoded `1` that ignored
//! `dpr`.
//!
//! ## Cold cache
//!
//! Each checkpoint gets its own freshly created, empty Chrome
//! `user_data_dir` (see [`launch`]) -- never reused across checkpoints or
//! runs -- so neither the wasm binary nor the font/panel-texture assets can
//! already be sitting in a disk cache from a previous capture. Every
//! checkpoint is a genuinely cold first load, not just the first checkpoint
//! of the scenario.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use headless_chrome::browser::tab::point::Point;
use headless_chrome::protocol::cdp::{Emulation, Page};
use headless_chrome::{Browser, LaunchOptionsBuilder, Tab};

/// Injected via `Page.addScriptToEvaluateOnNewDocument` (see [`launch`]), so
/// it runs before any of the game's own scripts on every document load and
/// is active from the very first frame. Captures every `console.*` call and
/// uncaught error/rejection into `window.__smoke`, purely from the browser
/// side -- the game's own Rust source is never touched to add a test hook
/// (see `xtask/Cargo.toml`'s dependency note: browser tooling stays in
/// `xtask`, never the game binary).
const INSTRUMENTATION_SCRIPT: &str = r#"
window.__smoke = { console: [], errors: [] };
// The modular paper-doll review loads hundreds of independent textures.
// Chrome's default 250-entry Resource Timing buffer evicts early requests
// before readiness checks can prove that their responses completed.
performance.setResourceTimingBufferSize(5000);
(function () {
  var push = function (level, args) {
    try {
      var parts = [];
      for (var i = 0; i < args.length; i++) {
        var a = args[i];
        try { parts.push(typeof a === 'string' ? a : JSON.stringify(a)); }
        catch (e) { parts.push(String(a)); }
      }
      window.__smoke.console.push(level + ': ' + parts.join(' '));
    } catch (e) {}
  };
  ['log', 'warn', 'error', 'info', 'debug'].forEach(function (level) {
    var orig = console[level] ? console[level].bind(console) : function () {};
    console[level] = function () {
      push(level, arguments);
      return orig.apply(console, arguments);
    };
  });
  window.addEventListener('error', function (e) {
    window.__smoke.errors.push('error: ' + (e && e.message ? e.message : String(e)));
  });
  window.addEventListener('unhandledrejection', function (e) {
    window.__smoke.errors.push('unhandledrejection: ' + String(e && e.reason));
  });
})();
"#;

/// Pulls one JSON status snapshot from the page: DOM/canvas readiness
/// signals, measured viewport/scroll geometry, and everything
/// `INSTRUMENTATION_SCRIPT` captured so far (console messages, errors, and
/// every sub-resource fetch the `Resource Timing` API observed -- used
/// instead of wiring up CDP's `Network` domain, since a plain `evaluate()`
/// round-trip already gives per-request status/size for free).
const STATUS_SCRIPT: &str = r#"
JSON.stringify({
  loading_gone: document.getElementById('loading') === null,
  canvas_present: !!document.getElementById('game-canvas'),
  canvas_w: (document.getElementById('game-canvas') || {}).width || 0,
  canvas_h: (document.getElementById('game-canvas') || {}).height || 0,
  inner_width: window.innerWidth,
  inner_height: window.innerHeight,
  device_pixel_ratio: window.devicePixelRatio,
  scroll_width: document.documentElement.scrollWidth,
  scroll_height: document.documentElement.scrollHeight,
  client_width: document.documentElement.clientWidth,
  client_height: document.documentElement.clientHeight,
  console: (window.__smoke || {}).console || [],
  errors: (window.__smoke || {}).errors || [],
  resources: performance.getEntriesByType('resource').map(function (e) {
    var status = typeof e.responseStatus === 'number' && e.responseStatus > 0
      ? e.responseStatus
      : (e.transferSize > 0 || e.decodedBodySize > 0 ? 200 : 0);
    return { url: e.name, status: status, transfer_size: (e.transferSize || e.decodedBodySize || 0) };
  })
})
"#;

/// One resource-timing entry observed by [`STATUS_SCRIPT`].
#[derive(serde::Deserialize, Debug, Clone)]
pub struct ResourceEntry {
    pub url: String,
    pub status: i64,
    pub transfer_size: f64,
}

/// One status snapshot; see [`STATUS_SCRIPT`] for exactly what each field
/// means.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct PageStatus {
    pub loading_gone: bool,
    pub canvas_present: bool,
    pub canvas_w: f64,
    pub canvas_h: f64,
    pub inner_width: f64,
    pub inner_height: f64,
    pub device_pixel_ratio: f64,
    pub scroll_width: f64,
    pub scroll_height: f64,
    pub client_width: f64,
    pub client_height: f64,
    pub console: Vec<String>,
    pub errors: Vec<String>,
    pub resources: Vec<ResourceEntry>,
}

impl PageStatus {
    /// The DOM/canvas readiness precondition every poll of the readiness
    /// loop (see `cold_menu::wait_until_ready_and_capture`) checks before
    /// it's willing to start looking at screenshot stability: the loading
    /// screen removed itself (Trunk's `TrunkApplicationStarted`, see
    /// `index.html`) and the canvas has a real backing size.
    pub fn app_booted(&self) -> bool {
        self.loading_gone && self.canvas_present && self.canvas_w > 0.0 && self.canvas_h > 0.0
    }
}

/// A launched checkpoint browser: one Chrome process (fresh profile) and
/// its one tab. Dropping this ends the Chrome process (`headless_chrome`'s
/// own `Browser` `Drop` impl closes it), so a checkpoint's browser is always
/// torn down when the function that launched it returns -- including on an
/// early `?` return from a failed assertion.
pub struct Checkpoint {
    #[allow(dead_code)] // kept alive so its `Drop` tears the process down with the tab
    browser: Browser,
    tab: Arc<Tab>,
}

/// Finds a Chrome/Chromium binary that actually runs.
///
/// `headless_chrome::browser::default_executable()` alone is not enough: it
/// returns the first *existing, executable* candidate (a `CHROME` env var,
/// then `chromium` etc. on `PATH`, then the standard `/Applications` install
/// paths), but never verifies the file launches. A stale Homebrew cask shim
/// -- e.g. `/opt/homebrew/bin/chromium` wrapping an uninstalled
/// `/Applications/Chromium.app` -- passes that check and then exits
/// instantly at spawn time, which `headless_chrome` surfaces (after ten
/// 30-second retry rounds) as a misleading "no available ports between
/// 8000 and 9000" error. So each candidate is validated by actually
/// executing `<binary> --version` and requiring success, falling back to
/// the standard macOS/Linux Chrome install locations if the
/// `default_executable` pick is broken.
fn chrome_binary() -> Result<std::path::PathBuf, String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(path) = headless_chrome::browser::default_executable() {
        candidates.push(path);
    }
    for fallback in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ] {
        let path = std::path::PathBuf::from(fallback);
        if path.exists() && !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    let mut rejected = Vec::new();
    for candidate in candidates {
        match std::process::Command::new(&candidate)
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("    browser: {} ({version})", candidate.display());
                return Ok(candidate);
            }
            Ok(output) => rejected.push(format!(
                "{} (--version exited {:?})",
                candidate.display(),
                output.status.code()
            )),
            Err(e) => rejected.push(format!("{} (failed to execute: {e})", candidate.display())),
        }
    }
    Err(format!(
        "no working Chrome/Chromium binary found; set the CHROME env var to one. Rejected candidates: [{}]",
        rejected.join(", ")
    ))
}

/// The fixed Chrome launch flags for one checkpoint, parameterized only by
/// `dpr` (#278) -- a pure function (returning owned `String`s, not
/// `&'static str`, since the scale-factor flag is now formatted) so the
/// exact flag text is unit-testable without spinning up a real Chrome
/// process. See [`launch`]'s doc comment for why
/// `--force-device-scale-factor` must track `dpr` instead of staying
/// hardcoded at `1`.
fn chrome_launch_args(dpr: f64) -> Vec<String> {
    vec![
        format!("--force-device-scale-factor={dpr}"),
        "--hide-scrollbars".to_string(),
        "--enable-unsafe-swiftshader".to_string(),
        "--use-angle=swiftshader".to_string(),
    ]
}

/// Launches a fresh, isolated headless Chrome against `profile_dir` (must
/// not exist yet or be empty -- callers create a new directory per
/// checkpoint, see `cold_menu::run_checkpoint`) sized to exactly
/// `width`x`height` logical (CSS) pixels at the given device pixel ratio
/// (`dpr`).
///
/// ## DPR emulation (#198, fixed by #278)
///
/// The Chrome launch flag is `--force-device-scale-factor={dpr}` -- pinning
/// the *real*, host-independent rendering/compositing scale this tab uses
/// to exactly the checkpoint's requested DPR (see [`chrome_launch_args`]).
/// CDP's `Emulation.setDeviceMetricsOverride` is layered on top with the
/// same `dpr` (below), which keeps the CSS-pixel viewport
/// (`window.innerWidth/innerHeight`) exactly `width`x`height` at every DPR
/// and is what `checkpoint::screenshot_png` (via `Page.captureScreenshot`'s
/// `clip`) and the page's own `window.devicePixelRatio` observe -- so the
/// captured screenshot's *physical* pixel dimensions scale to
/// `width*dpr`x`height*dpr`, asserted by each scenario's
/// `check_screenshot_pixels`.
///
/// Before #278's fix, the launch flag stayed hardcoded at
/// `--force-device-scale-factor=1` regardless of `dpr`, on the assumption
/// that the CDP override alone "supersedes the launch-time default" for
/// every DPR-dependent observable. That assumption held for
/// `window.devicePixelRatio` and screenshot scaling, but **not** for the
/// wasm canvas's actual backing-store resolution: winit's web backend reads
/// `scale_factor` from `window.devicePixelRatio` (CDP-overridden) but
/// derives the physical size it reports to Bevy from a `ResizeObserver`
/// `devicePixelContentBoxSize` measurement of the canvas, which tracks the
/// tab's *real* compositor scale -- pinned to `1` by the old hardcoded
/// launch flag, independent of the CDP override. At a "desktop"
/// 1280x800 DPR-2/3 checkpoint that meant Bevy's window resolution divided
/// a real 1280-physical-px canvas by a CDP-reported `scale_factor` of 2 or
/// 3, landing on a ~427-640-logical-px width -- under `theme::MOBILE_BREAKPOINT`
/// (700) -- so the whole desktop UI (fight action palette, shop layout,
/// ...) rendered its mobile variant, oversized to fill the real 1280x800
/// canvas. Pinning the launch-time flag itself to `dpr` keeps the real
/// compositor scale and the CDP-reported `scale_factor` in agreement again,
/// exactly as they always are on a real, unemulated browser.
pub fn launch(width: u32, height: u32, dpr: f64, profile_dir: &Path) -> Result<Checkpoint, String> {
    std::fs::create_dir_all(profile_dir).map_err(|e| {
        format!(
            "failed to create Chrome profile dir {}: {e}",
            profile_dir.display()
        )
    })?;
    let chrome_path = chrome_binary()?;

    let args_owned = chrome_launch_args(dpr);
    let args: Vec<&OsStr> = args_owned.iter().map(OsStr::new).collect();

    let launch_options = LaunchOptionsBuilder::default()
        .headless(true)
        .sandbox(false) // required in CI/sandboxed dev containers lacking user namespaces (see PR description)
        // `headless_chrome` adds `--disable-gpu` unless told otherwise, which
        // disables the GPU process entirely -- but software WebGL (ANGLE via
        // swiftshader, forced above) still runs *inside* that process, so
        // `--disable-gpu` made Chrome's GPU process (and the tab with it)
        // die almost immediately. Keeping the GPU process enabled while
        // forcing swiftshader is what actually gives deterministic, working
        // software-rendered WebGL in headless mode.
        .enable_gpu(true)
        .path(Some(chrome_path))
        .user_data_dir(Some(profile_dir.to_path_buf()))
        .window_size(Some((width, height)))
        .idle_browser_timeout(Duration::from_secs(120)) // a cold wasm compile+first-fetch can be slow
        .args(args)
        .build()
        .map_err(|e| format!("failed to build Chrome launch options: {e}"))?;

    let browser =
        Browser::new(launch_options).map_err(|e| format!("failed to launch Chrome: {e}"))?;
    let tab = browser
        .new_tab()
        .map_err(|e| format!("failed to open a browser tab: {e}"))?;

    // `--window-size` alone is not trustworthy for the *viewport*: headless
    // Chrome still applies window-chrome/minimum-size rules (observed on
    // macOS: a requested 390x844 window yields a 500x705 viewport). Override
    // the device metrics explicitly so `window.innerWidth/innerHeight` are
    // exactly the checkpoint's requested size at DPR 1; the readiness loop's
    // status snapshot then *verifies* the override took (see
    // `cold_menu::check_no_unexpected_scroll`'s viewport assertions).
    tab.call_method(Emulation::SetDeviceMetricsOverride {
        width,
        height,
        device_scale_factor: dpr,
        mobile: false,
        scale: None,
        screen_width: None,
        screen_height: None,
        position_x: None,
        position_y: None,
        dont_set_visible_size: None,
        screen_orientation: None,
        viewport: None,
        display_feature: None,
        device_posture: None,
    })
    .map_err(|e| format!("failed to override device metrics to {width}x{height}@{dpr}x: {e}"))?;

    tab.call_method(Page::AddScriptToEvaluateOnNewDocument {
        source: INSTRUMENTATION_SCRIPT.to_string(),
        world_name: None,
        include_command_line_api: None,
        run_immediately: None,
    })
    .map_err(|e| format!("failed to install console/error instrumentation: {e}"))?;

    apply_cpu_throttle(&tab)?;

    Ok(Checkpoint { browser, tab })
}

/// Env var read by [`apply_cpu_throttle`]. Unset/unparseable/`<= 1` disables
/// throttling entirely (the default) so normal local runs and CI are
/// unaffected -- this exists purely as a *local repro* tool for the class of
/// slow-CI timing race documented on `ui_widgets::focus::PendingFocusNav`:
/// headless CI (real browser, `SwiftShader` software rendering, a shared
/// runner) is measurably slower per-frame than a dev laptop, which is
/// invisible locally unless the tab is deliberately slowed down to match.
/// Kept as a permanent, off-by-default harness feature (rather than reverted
/// after #268) because this class of "app assumes its target entity exists
/// the frame it runs" bug has recurred more than once across this scenario's
/// screens -- see #268's PR history -- and a fast dev machine alone cannot
/// reproduce it.
///
/// Example: `XTASK_WEB_SMOKE_CPU_THROTTLE=20 cargo xtask web-smoke --scenario
/// keyboard-accessibility` emulates a CPU roughly 20x slower than this
/// machine's for the whole session (every checkpoint launched via [`launch`]
/// picks up its own tab-local override).
const CPU_THROTTLE_ENV_VAR: &str = "XTASK_WEB_SMOKE_CPU_THROTTLE";

/// Applies [`CPU_THROTTLE_ENV_VAR`] (if set to a valid rate `> 1`) to `tab`
/// via CDP `Emulation.setCPUThrottlingRate` -- see that constant's doc
/// comment for why this exists and defaults off. `rate` is Chrome's own
/// slowdown factor (`1` = no throttle, `20` = roughly 20x slower), applied
/// once per launched tab, before the caller ever navigates -- so it's in
/// effect from the very first frame of the cold boot this harness's
/// readiness loop (`cold_menu`'s doc comment) waits out, the same window the
/// documented timing races live in.
fn apply_cpu_throttle(tab: &Tab) -> Result<(), String> {
    let Ok(raw) = std::env::var(CPU_THROTTLE_ENV_VAR) else {
        return Ok(());
    };
    let Ok(rate) = raw.parse::<f64>() else {
        eprintln!(
            "    {CPU_THROTTLE_ENV_VAR}={raw:?} is not a valid number; ignoring (no throttle applied)"
        );
        return Ok(());
    };
    if rate <= 1.0 {
        return Ok(());
    }
    tab.call_method(Emulation::SetCPUThrottlingRate { rate })
        .map_err(|e| format!("failed to set CPU throttling rate to {rate}x: {e}"))?;
    println!("    CPU throttling: {rate}x slower ({CPU_THROTTLE_ENV_VAR})");
    Ok(())
}

impl Checkpoint {
    /// Registers a `localStorage.setItem(key, value)` call to run before any
    /// of the page's own scripts on the *next* navigation (via the same
    /// `Page.AddScriptToEvaluateOnNewDocument` mechanism [`launch`] already
    /// uses for `INSTRUMENTATION_SCRIPT`). Used by the `reduced-motion-fight`
    /// scenario (#200) to seed the persisted `rff_settings_v1` blob with
    /// `reduced_motion: true` *before* the wasm app's `Startup` schedule
    /// runs `settings::load_settings` -- reading `localStorage` after the
    /// fact would be too late, since the preference must already be applied
    /// when the arena's motion systems spawn.
    pub fn seed_local_storage_before_load(&self, key: &str, value: &str) -> Result<(), String> {
        // `{key:?}`/`{value:?}` render as double-quoted, backslash-escaped
        // Rust string literals, which are also valid JS string literals --
        // the same trick `gold_journey::send_command` uses for its command
        // payload.
        let script = format!("try {{ localStorage.setItem({key:?}, {value:?}); }} catch (e) {{}}");
        self.tab
            .call_method(Page::AddScriptToEvaluateOnNewDocument {
                source: script,
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            })
            .map_err(|e| format!("failed to seed localStorage[{key:?}] before load: {e}"))?;
        Ok(())
    }

    pub fn navigate(&self, url: &str) -> Result<(), String> {
        self.tab
            .navigate_to(url)
            .map_err(|e| format!("navigation to {url} failed: {e}"))?;
        Ok(())
    }

    /// Reloads the current page (`Shift+F5`-style: cache not ignored, so the
    /// already-cached wasm/asset bytes are reused -- only the running app
    /// state is discarded), used by the `accessibility-settings-reload`
    /// scenario (#191) to prove a stored preference survives a real browser
    /// reload, not just an in-memory resource mutation.
    pub fn reload(&self) -> Result<(), String> {
        self.tab
            .reload(false, None)
            .map_err(|e| format!("page reload failed: {e}"))?;
        Ok(())
    }

    /// Clicks the page at exact CSS-pixel coordinates (viewport space, DPR 1
    /// per [`launch`]'s device-metrics override) via a real CDP mouse
    /// move+press+release -- not a JS-synthesized event -- so it exercises
    /// the same input path a real user's click does. The game's UI is
    /// entirely canvas-rendered (`bevy_ui`), so there is no DOM element to
    /// address; callers locate a button's center pixel themselves (see
    /// `accessibility_settings_reload::find_wide_button_centers`, which
    /// scans a screenshot for the game's known solid button color instead).
    pub fn click(&self, x: f64, y: f64) -> Result<(), String> {
        self.tab
            .click_point(Point { x, y })
            .map_err(|e| format!("click at ({x}, {y}) failed: {e}"))?;
        Ok(())
    }

    /// Presses (and releases) one keyboard key via a real CDP
    /// `Input.dispatchKeyEvent` pair -- not a JS-synthesized `KeyboardEvent`
    /// -- so it exercises the same input path a real keypress does, all the
    /// way through the browser's own keyboard pipeline into the game's wasm
    /// winit backend. Used by the `fight-palette-accessible` scenario (#213)
    /// to drive keyboard focus navigation/activation exactly the way a real
    /// player's keyboard would, rather than seeding `Interaction`/resource
    /// state through the review seam the way `pressButton`/
    /// `pressActionCategory` do for pointer input. `key` is a
    /// puppeteer-style key name (`"Tab"`, `"Enter"`, `" "` for Space,
    /// `"ArrowRight"`, ...); see [`headless_chrome::Tab::press_key`]'s docs
    /// for the full table.
    pub fn press_key(&self, key: &str) -> Result<(), String> {
        self.tab
            .press_key(key)
            .map_err(|e| format!("pressing key {key:?} failed: {e}"))?;
        Ok(())
    }

    /// Awaits one real rendered frame (`requestAnimationFrame`, resolved as
    /// a promise `evaluate` blocks on) -- the readiness loop's unit of
    /// waiting, in place of a wall-clock sleep: see `cold_menu`'s module
    /// docs for the full readiness contract this is one building block of.
    pub fn wait_for_frame(&self) -> Result<(), String> {
        self.tab
            .evaluate(
                "new Promise(function (resolve) { requestAnimationFrame(function () { resolve(true); }); })",
                true,
            )
            .map_err(|e| format!("waiting for an animation frame failed: {e}"))?;
        Ok(())
    }

    pub fn read_status(&self) -> Result<PageStatus, String> {
        self.eval_json(STATUS_SCRIPT)
    }

    /// Evaluates `script` -- which must, like [`STATUS_SCRIPT`], end in a
    /// `JSON.stringify(...)` of its own result -- and parses that JSON into
    /// `T`. The generic building block behind [`Checkpoint::read_status`];
    /// a scenario needing a different JSON shape (e.g.
    /// `accessibility_settings_reload`'s viewport-zoom capability and
    /// `localStorage` inspection) calls this directly instead of growing
    /// [`PageStatus`] with fields only it needs.
    pub fn eval_json<T: serde::de::DeserializeOwned>(&self, script: &str) -> Result<T, String> {
        let remote = self
            .tab
            .evaluate(script, false)
            .map_err(|e| format!("evaluating script failed: {e}"))?;
        let json = remote
            .value
            .ok_or_else(|| "script evaluation returned no value".to_string())?;
        let json_str: String = serde_json::from_value(json)
            .map_err(|e| format!("evaluated value was not a string: {e}"))?;
        serde_json::from_str(&json_str)
            .map_err(|e| format!("evaluated value was not valid JSON: {e}"))
    }

    /// Evaluates an arbitrary JS statement/expression, discarding any
    /// result. Used by the `gold-journey` scenario (#187) to write review
    /// commands into `localStorage` (see `src/review/mod.rs`'s module docs
    /// for the seam this drives) -- `cold-menu` has no analogous need since
    /// it never interacts with the page beyond reading status.
    pub fn eval_unit(&self, script: &str) -> Result<(), String> {
        self.tab
            .evaluate(script, false)
            .map_err(|e| format!("eval failed: {e}"))?;
        Ok(())
    }

    /// Evaluates a JS expression and returns its result as a string, if any
    /// (`null`/`undefined`/non-string results read back as `None`). Used by
    /// `gold-journey` (#187) to poll the review seam's published
    /// `localStorage` screen marker.
    pub fn eval_string(&self, script: &str) -> Result<Option<String>, String> {
        let remote = self
            .tab
            .evaluate(script, false)
            .map_err(|e| format!("eval failed: {e}"))?;
        Ok(remote.value.and_then(|v| v.as_str().map(str::to_string)))
    }

    /// Captures a PNG screenshot of exactly the `width`x`height` viewport
    /// (CDP's `Page.captureScreenshot` `clip`, not a JS canvas readback --
    /// this captures the actual compositor output regardless of whether
    /// Bevy's WebGL context was created with `preserveDrawingBuffer`, which
    /// this harness does not control).
    pub fn screenshot_png(&self, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let clip = Page::Viewport {
            x: 0.0,
            y: 0.0,
            width: f64::from(width),
            height: f64::from(height),
            scale: 1.0,
        };
        self.tab
            .capture_screenshot(
                Page::CaptureScreenshotFormatOption::Png,
                None,
                Some(clip),
                true,
            )
            .map_err(|e| format!("screenshot capture failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #278 root cause, pinned: before this fix,
    /// `--force-device-scale-factor` stayed hardcoded at `1` regardless of
    /// `dpr`, on the (false) assumption that CDP's
    /// `Emulation.setDeviceMetricsOverride` alone "supersedes the
    /// launch-time default" for every DPR-dependent observable -- see
    /// [`launch`]'s doc comment for the full mechanism this pins. This test
    /// fails against the pre-fix hardcoded `"1"` for every `dpr != 1.0`.
    #[test]
    fn launch_args_pin_the_real_device_scale_factor_to_the_requested_dpr() {
        for dpr in [1.0, 2.0, 3.0] {
            let args = chrome_launch_args(dpr);
            assert!(
                args.contains(&format!("--force-device-scale-factor={dpr}")),
                "dpr {dpr}: expected an explicit --force-device-scale-factor matching \
                 the checkpoint's requested DPR, got {args:?}"
            );
        }
    }

    /// The determinism-motivated flags (`browser`'s module docs,
    /// "Determinism") must survive alongside the now-`dpr`-dependent scale
    /// factor -- regressing any of these would reintroduce host-dependent,
    /// non-reproducible screenshots.
    #[test]
    fn launch_args_always_force_software_rendering() {
        for dpr in [1.0, 2.0, 3.0] {
            let args = chrome_launch_args(dpr);
            assert!(args.contains(&"--use-angle=swiftshader".to_string()));
            assert!(args.contains(&"--enable-unsafe-swiftshader".to_string()));
            assert!(args.contains(&"--hide-scrollbars".to_string()));
        }
    }
}
