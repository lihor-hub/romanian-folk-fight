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
//! forces `--force-device-scale-factor=1` so a Retina/HiDPI dev machine
//! doesn't silently double the captured pixel dimensions relative to a CI
//! runner (the issue requires DPR 1 at both checkpoint sizes).
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

/// Launches a fresh, isolated headless Chrome against `profile_dir` (must
/// not exist yet or be empty -- callers create a new directory per
/// checkpoint, see `cold_menu::run_checkpoint`) sized to exactly
/// `width`x`height` logical pixels at DPR 1.
pub fn launch(width: u32, height: u32, profile_dir: &Path) -> Result<Checkpoint, String> {
    std::fs::create_dir_all(profile_dir).map_err(|e| {
        format!(
            "failed to create Chrome profile dir {}: {e}",
            profile_dir.display()
        )
    })?;
    let chrome_path = chrome_binary()?;

    let args: Vec<&OsStr> = vec![
        OsStr::new("--force-device-scale-factor=1"),
        OsStr::new("--hide-scrollbars"),
        OsStr::new("--enable-unsafe-swiftshader"),
        OsStr::new("--use-angle=swiftshader"),
    ];

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
        device_scale_factor: 1.0,
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
    .map_err(|e| format!("failed to override device metrics to {width}x{height}@1: {e}"))?;

    tab.call_method(Page::AddScriptToEvaluateOnNewDocument {
        source: INSTRUMENTATION_SCRIPT.to_string(),
        world_name: None,
        include_command_line_api: None,
        run_immediately: None,
    })
    .map_err(|e| format!("failed to install console/error instrumentation: {e}"))?;

    Ok(Checkpoint { browser, tab })
}

impl Checkpoint {
    pub fn navigate(&self, url: &str) -> Result<(), String> {
        self.tab
            .navigate_to(url)
            .map_err(|e| format!("navigation to {url} failed: {e}"))?;
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
        let remote = self
            .tab
            .evaluate(STATUS_SCRIPT, false)
            .map_err(|e| format!("reading page status failed: {e}"))?;
        let json = remote
            .value
            .ok_or_else(|| "page status evaluation returned no value".to_string())?;
        let json_str: String = serde_json::from_value(json)
            .map_err(|e| format!("page status value was not a string: {e}"))?;
        serde_json::from_str(&json_str).map_err(|e| format!("page status was not valid JSON: {e}"))
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
