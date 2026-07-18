//! The `hybrid-2-5d-character` browser scenario (#321).
//!
//! One continuous session per viewport proves that the same representative
//! human identity and 15-part cutout silhouette survive creation, shop, and
//! combat. Desktop and phone each finish on a captured hybrid-material combat
//! baseline. A final desktop-only phase serves the same review build with the
//! optional mask/normal/shadow files withheld, proving that every material-
//! capable part remains visibly represented by its deterministic albedo
//! `Sprite` without changing identity or silhouette facts.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "hybrid-2-5d-character";

const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
const REVIEW_HYBRID_CHARACTER_KEY: &str = "rff_review_hybrid_character_v1";

const REPRESENTATIVE_PRESET: &str = "Ucenicul Solomonar";
const COMBAT_SEED: u64 = 20;
const EXPECTED_STABLE_IDS: &[&str] = &[
    "human.body.foundation.v1",
    "human.face.default.v1",
    "human.hair.braided.v1",
    "human.torso.linen.v1",
    "human.legs.itari.v1",
    "human.feet.opinci.v1",
];
const EXPECTED_PART_COUNT: usize = 15;
const EXPECTED_MATERIAL_PART_COUNT: usize = 6;

const MAX_FRAMES: usize = 3600;
const MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;
const BASELINE_TIME_SECONDS: f32 = 10_000.0;

#[derive(Debug, Clone, Copy)]
struct Viewport {
    name: &'static str,
    width: u32,
    height: u32,
}

const VIEWPORTS: &[Viewport] = &[
    Viewport {
        name: "desktop",
        width: 1440,
        height: 900,
    },
    Viewport {
        name: "phone",
        width: 390,
        height: 844,
    },
];

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
struct HybridCharacterSnapshot {
    screen: String,
    root_entity: String,
    selected_part_ids: Vec<String>,
    part_count: usize,
    material_part_count: usize,
    render_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedRenderPath {
    Hybrid,
    Fallback,
}

impl ExpectedRenderPath {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid_material",
            Self::Fallback => "albedo_fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedStage {
    Creation,
    Shop,
    Fight,
}

impl ExpectedStage {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Creation => "CharacterCreation",
            Self::Shop => "Shop",
            Self::Fight => "Fight",
        }
    }
}

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let mut missing_baseline = false;

    {
        let server = start_server(&dist_dir, "hybrid inputs")?;
        for viewport in VIEWPORTS {
            run_journey(
                viewport,
                &server,
                ExpectedRenderPath::Hybrid,
                true,
                update_baselines,
                strict_visual,
                &mut missing_baseline,
            )?;
        }
    }

    disable_optional_material_inputs(&dist_dir).map_err(|error| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: disable optional material inputs"),
            error,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let fallback_server = start_server(&dist_dir, "forced fallback")?;
    run_journey(
        &Viewport {
            name: "fallback-desktop",
            width: 1440,
            height: 900,
        },
        &fallback_server,
        ExpectedRenderPath::Fallback,
        false,
        false,
        false,
        &mut missing_baseline,
    )?;

    if update_baselines {
        println!(
            "\n{SCENARIO}: updated {} accepted hybrid-material baselines under tests/visual/baselines/{SCENARIO}/.",
            VIEWPORTS.len()
        );
    } else if missing_baseline {
        println!(
            "\n{SCENARIO}: assertions passed, but an accepted viewport baseline is missing; review artifacts and rerun with --update-baselines."
        );
    } else {
        println!(
            "\n{SCENARIO}: desktop+phone hybrid baselines and forced albedo fallback all passed."
        );
    }
    Ok(())
}

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut command = Command::new("trunk");
    command
        .arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-hybrid-2-5d-character");
    command.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (hybrid-2-5d-character)",
        command,
    )?;
    Ok(workspace_root().join("dist-hybrid-2-5d-character"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always lives directly under the workspace root")
        .to_path_buf()
}

fn start_server(dist_dir: &Path, phase: &str) -> Result<StaticServer, SmokeError> {
    let server = StaticServer::start(dist_dir.to_path_buf()).map_err(|error| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve {phase}"),
            error,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!("{SCENARIO}: serving {phase} at {}", server.base_url());
    Ok(server)
}

/// Removes only generated optional channels from this scenario's disposable
/// Trunk output. Albedos and source assets remain untouched; every failed
/// channel load therefore exercises production's pending-sprite fallback.
fn disable_optional_material_inputs(dist_dir: &Path) -> Result<(), String> {
    let runtime = dist_dir.join("assets/fighters/human/runtime");
    let entries = std::fs::read_dir(&runtime)
        .map_err(|error| format!("could not read {}: {error}", runtime.display()))?;
    let mut removed = 0usize;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if is_optional_material_channel(name) {
            std::fs::remove_file(&path)
                .map_err(|error| format!("could not remove {}: {error}", path.display()))?;
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(format!(
            "no optional material channels existed under {}",
            runtime.display()
        ));
    }
    println!("{SCENARIO}: forced fallback withheld {removed} optional channel files");
    Ok(())
}

fn is_optional_material_channel(name: &str) -> bool {
    name.ends_with("_mask.png") || name.ends_with("_normal.png") || name.ends_with("_shadow.png")
}

#[allow(clippy::too_many_arguments)]
fn run_journey(
    viewport: &Viewport,
    server: &StaticServer,
    expected_path: ExpectedRenderPath,
    compare_baseline: bool,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let phase_dir = artifacts::scenario_dir(SCENARIO).join(viewport.name);
    let profile_dir = phase_dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);
    let checkpoint = browser::launch(viewport.width, viewport.height, 1.0, &profile_dir)
        .map_err(|error| smoke(viewport, "browser launch", error, &phase_dir))?;
    checkpoint
        .navigate(&format!("{}/", server.base_url()))
        .map_err(|error| smoke(viewport, "navigation", error, &phase_dir))?;

    wait_for_screen(&checkpoint, "MainMenu")
        .map_err(|error| smoke(viewport, "main menu", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "seedCombat", "seed": COMBAT_SEED}),
    )
    .map_err(|error| smoke(viewport, "seed combat", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
    )
    .map_err(|error| smoke(viewport, "start new game", error, &phase_dir))?;

    wait_for_screen(&checkpoint, "CharacterCreation")
        .map_err(|error| smoke(viewport, "creation screen", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "selectPreset", "preset": REPRESENTATIVE_PRESET}),
    )
    .map_err(|error| smoke(viewport, "select representative", error, &phase_dir))?;
    let creation = wait_for_snapshot(&checkpoint, expected_path, ExpectedStage::Creation, &[])
        .map_err(|error| smoke(viewport, "creation snapshot", error, &phase_dir))?;

    // Freeze before confirming the hero. State transitions and OnEnter setup
    // still run, while arena motion/AI cannot advance before the fresh first
    // fight baseline is captured. This pins a deterministic full-health idle
    // frame instead of a wall-clock-dependent later combat action.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimeElapsed", "seconds": BASELINE_TIME_SECONDS}),
    )
    .map_err(|error| {
        smoke(
            viewport,
            "set deterministic combat phase",
            error,
            &phase_dir,
        )
    })?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )
    .map_err(|error| smoke(viewport, "freeze before first combat", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )
    .map_err(|error| smoke(viewport, "confirm representative", error, &phase_dir))?;
    wait_for_screen(&checkpoint, "Fight")
        .map_err(|error| smoke(viewport, "first fight", error, &phase_dir))?;
    let first_combat = wait_for_snapshot(
        &checkpoint,
        expected_path,
        ExpectedStage::Fight,
        &[&creation.root_entity],
    )
    .map_err(|error| smoke(viewport, "first combat snapshot", error, &phase_dir))?;
    if let Some(problem) = identity_or_silhouette_changed(&creation, &first_combat) {
        return Err(smoke(
            viewport,
            "first combat identity",
            problem,
            &phase_dir,
        ));
    }
    let (status, screenshot) = capture_stable(&checkpoint, viewport)
        .map_err(|error| smoke(viewport, "stable capture", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )
    .map_err(|error| smoke(viewport, "unfreeze first combat", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
    )
    .map_err(|error| smoke(viewport, "enable autoplay", error, &phase_dir))?;
    wait_for_screen(&checkpoint, "FightResult")
        .map_err(|error| smoke(viewport, "first fight result", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
    )
    .map_err(|error| smoke(viewport, "go to shop", error, &phase_dir))?;

    wait_for_screen(&checkpoint, "Shop")
        .map_err(|error| smoke(viewport, "shop screen", error, &phase_dir))?;
    let shop = wait_for_snapshot(
        &checkpoint,
        expected_path,
        ExpectedStage::Shop,
        &[&creation.root_entity, &first_combat.root_entity],
    )
    .map_err(|error| smoke(viewport, "shop snapshot", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": false}),
    )
    .map_err(|error| smoke(viewport, "disable autoplay", error, &phase_dir))?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "BackToArena"}),
    )
    .map_err(|error| smoke(viewport, "leave shop", error, &phase_dir))?;

    wait_for_screen(&checkpoint, "Fight")
        .map_err(|error| smoke(viewport, "combat screen", error, &phase_dir))?;
    let combat = wait_for_snapshot(
        &checkpoint,
        expected_path,
        ExpectedStage::Fight,
        &[
            &creation.root_entity,
            &first_combat.root_entity,
            &shop.root_entity,
        ],
    )
    .map_err(|error| smoke(viewport, "combat snapshot", error, &phase_dir))?;
    for (screen, snapshot) in [("shop", &shop), ("combat", &combat)] {
        if let Some(problem) = identity_or_silhouette_changed(&creation, snapshot) {
            return Err(smoke(viewport, screen, problem, &phase_dir));
        }
    }

    write_artifacts(
        viewport,
        &status,
        &screenshot,
        &combat,
        server,
        compare_baseline,
    );
    assert_capture(viewport, &status, &screenshot, expected_path)
        .map_err(|error| smoke(viewport, "capture assertions", error, &phase_dir))?;
    if compare_baseline {
        handle_baseline(
            viewport,
            &screenshot,
            update_baselines,
            strict_visual,
            missing_baseline,
        )?;
    }

    println!(
        "{SCENARIO}[{}]: creation -> shop -> combat retained {} exact IDs / {} parts via {}",
        viewport.name,
        combat.selected_part_ids.len(),
        combat.part_count,
        combat.render_path
    );
    Ok(())
}

fn smoke(viewport: &Viewport, step: &str, error: String, artifacts_dir: &Path) -> SmokeError {
    SmokeError::scenario(
        format!("web-smoke {SCENARIO}[{}]: {step}", viewport.name),
        error,
        artifacts_dir.to_path_buf(),
    )
}

const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;

fn send_command(checkpoint: &Checkpoint, payload: serde_json::Value) -> Result<(), String> {
    let json = payload.to_string();
    let literal = serde_json::to_string(&json).map_err(|error| error.to_string())?;
    checkpoint.eval_unit(&format!(
        "localStorage.setItem('{REVIEW_COMMAND_KEY}', {literal});"
    ))?;
    for _ in 0..COMMAND_CONSUMED_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if checkpoint
            .eval_string(&format!("localStorage.getItem('{REVIEW_COMMAND_KEY}')"))?
            .is_none()
        {
            return Ok(());
        }
    }
    Err(format!("review command was not consumed: {json}"))
}

fn wait_for_screen(checkpoint: &Checkpoint, expected: &str) -> Result<(), String> {
    let start = Instant::now();
    let mut last = None;
    for _ in 0..MAX_FRAMES {
        if start.elapsed() > MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        last = checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))?;
        if last.as_deref() == Some(expected) && checkpoint.read_status()?.app_booted() {
            return Ok(());
        }
    }
    Err(format!(
        "never observed screen {expected:?} within {MAX_FRAMES} frames/{MAX_WALL_CLOCK:?}; last={last:?}"
    ))
}

fn read_snapshot(checkpoint: &Checkpoint) -> Result<Option<HybridCharacterSnapshot>, String> {
    let Some(json) = checkpoint.eval_string(&format!(
        "localStorage.getItem('{REVIEW_HYBRID_CHARACTER_KEY}')"
    ))?
    else {
        return Ok(None);
    };
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|error| format!("invalid hybrid-character snapshot {json}: {error}"))
}

fn wait_for_snapshot(
    checkpoint: &Checkpoint,
    expected_path: ExpectedRenderPath,
    expected_stage: ExpectedStage,
    rejected_roots: &[&str],
) -> Result<HybridCharacterSnapshot, String> {
    let start = Instant::now();
    let mut last = None;
    for _ in 0..MAX_FRAMES {
        if start.elapsed() > MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_snapshot(checkpoint)? {
            if snapshot_problem(&snapshot, expected_path, expected_stage, rejected_roots).is_none()
            {
                return Ok(snapshot);
            }
            last = Some(snapshot);
        }
    }
    Err(format!(
        "snapshot never reached {} / {} with a fresh root and exact identity/silhouette; last={last:?}",
        expected_stage.wire_name(),
        expected_path.wire_name(),
    ))
}

fn snapshot_problem(
    snapshot: &HybridCharacterSnapshot,
    expected_path: ExpectedRenderPath,
    expected_stage: ExpectedStage,
    rejected_roots: &[&str],
) -> Option<String> {
    if snapshot.screen != expected_stage.wire_name() {
        return Some(format!(
            "snapshot screen {:?}, expected {:?}",
            snapshot.screen,
            expected_stage.wire_name()
        ));
    }
    if rejected_roots.contains(&snapshot.root_entity.as_str()) {
        return Some(format!(
            "snapshot root {:?} is stale from the previous stage",
            snapshot.root_entity
        ));
    }
    let expected_ids: Vec<String> = EXPECTED_STABLE_IDS
        .iter()
        .map(ToString::to_string)
        .collect();
    if snapshot.selected_part_ids != expected_ids {
        return Some(format!(
            "selected IDs {:?}, expected {expected_ids:?}",
            snapshot.selected_part_ids
        ));
    }
    if snapshot.part_count != EXPECTED_PART_COUNT {
        return Some(format!(
            "part count {}, expected {EXPECTED_PART_COUNT}",
            snapshot.part_count
        ));
    }
    if snapshot.material_part_count != EXPECTED_MATERIAL_PART_COUNT {
        return Some(format!(
            "material part count {}, expected {EXPECTED_MATERIAL_PART_COUNT}",
            snapshot.material_part_count
        ));
    }
    if snapshot.render_path != expected_path.wire_name() {
        return Some(format!(
            "render path {:?}, expected {:?}",
            snapshot.render_path,
            expected_path.wire_name()
        ));
    }
    None
}

fn identity_or_silhouette_changed(
    expected: &HybridCharacterSnapshot,
    actual: &HybridCharacterSnapshot,
) -> Option<String> {
    (expected.selected_part_ids != actual.selected_part_ids
        || expected.part_count != actual.part_count
        || expected.material_part_count != actual.material_part_count)
        .then(|| format!("identity/silhouette changed: expected={expected:?}, actual={actual:?}"))
}

fn capture_stable(
    checkpoint: &Checkpoint,
    viewport: &Viewport,
) -> Result<(PageStatus, Vec<u8>), String> {
    let mut previous = None;
    let mut stable = 0usize;
    for _ in 0..300 {
        checkpoint.wait_for_frame()?;
        let screenshot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
        stable = if previous.as_deref() == Some(screenshot.as_slice()) {
            stable + 1
        } else {
            1
        };
        previous = Some(screenshot);
        if stable >= STABLE_FRAMES_REQUIRED {
            return Ok((
                checkpoint.read_status()?,
                previous.expect("the stable frame was just captured"),
            ));
        }
    }
    Err("combat screenshot did not stabilize after pausing virtual time".to_owned())
}

fn assert_capture(
    viewport: &Viewport,
    status: &PageStatus,
    screenshot: &[u8],
    expected_path: ExpectedRenderPath,
) -> Result<(), String> {
    if !status.errors.is_empty() {
        return Err(format!("page errors: {:?}", status.errors));
    }
    let errors = unexpected_console_errors(&status.console, expected_path);
    if !errors.is_empty() {
        return Err(format!("unexpected console errors: {errors:?}"));
    }
    if status.inner_width != f64::from(viewport.width)
        || status.inner_height != f64::from(viewport.height)
        || status.scroll_width > status.client_width + 1.0
        || status.scroll_height > status.client_height + 1.0
    {
        return Err(format!(
            "unexpected viewport/scroll: inner={}x{}, client={}x{}, scroll={}x{}",
            status.inner_width,
            status.inner_height,
            status.client_width,
            status.client_height,
            status.scroll_width,
            status.scroll_height
        ));
    }
    let image = image::load_from_memory(screenshot)
        .map_err(|error| format!("screenshot was not PNG: {error}"))?;
    if image.width() != viewport.width || image.height() != viewport.height {
        return Err(format!(
            "screenshot was {}x{}, expected {}x{}",
            image.width(),
            image.height(),
            viewport.width,
            viewport.height
        ));
    }
    Ok(())
}

fn unexpected_console_errors(console: &[String], expected_path: ExpectedRenderPath) -> Vec<String> {
    console
        .iter()
        .filter(|line| is_console_error(line))
        .filter(|line| {
            expected_path != ExpectedRenderPath::Fallback
                || !is_expected_missing_material_channel_error(line)
        })
        .cloned()
        .collect()
}

fn is_console_error(line: &str) -> bool {
    line.starts_with("error:") || line.contains("%cERROR%c")
}

fn is_expected_missing_material_channel_error(line: &str) -> bool {
    const PREFIX: &str = "Path not found: assets/fighters/human/runtime/";
    let Some((_, remainder)) = line.split_once(PREFIX) else {
        return false;
    };
    let Some(file_name) = remainder.split_whitespace().next() else {
        return false;
    };
    !file_name.contains('/') && is_optional_material_channel(file_name)
}

fn write_artifacts(
    viewport: &Viewport,
    status: &PageStatus,
    screenshot: &[u8],
    snapshot: &HybridCharacterSnapshot,
    server: &StaticServer,
    baseline_capture: bool,
) {
    let key = if baseline_capture {
        viewport.name.to_owned()
    } else {
        "fallback".to_owned()
    };
    let Ok(dir) = artifacts::checkpoint_dir(SCENARIO, &key) else {
        return;
    };
    let _ = artifacts::write_artifact(&dir, "screenshot.png", screenshot);
    let _ = artifacts::write_artifact(&dir, "console.log", status.console.join("\n"));
    let _ = artifacts::write_artifact(
        &dir,
        "snapshot.json",
        serde_json::to_string_pretty(snapshot).unwrap_or_default(),
    );
    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
}

fn handle_baseline(
    viewport: &Viewport,
    screenshot: &[u8],
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::scenario_dir(SCENARIO).join(viewport.name);
    match baseline::handle(SCENARIO, viewport.name, screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => println!(
            "{SCENARIO}[{}]: baseline updated at {}",
            viewport.name,
            baseline::baseline_path(SCENARIO, viewport.name).display()
        ),
        Ok(baseline::BaselineOutcome::Matches) => {
            println!("{SCENARIO}[{}]: matches accepted baseline", viewport.name)
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            if strict_visual {
                return Err(smoke(
                    viewport,
                    "strict visual",
                    "accepted baseline is missing".to_owned(),
                    &dir,
                ));
            }
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff = baseline::write_diff_triplet(SCENARIO, viewport.name, screenshot, &dir);
            if strict_visual {
                let suffix = diff
                    .map(|paths| format!("; {}", paths.describe()))
                    .unwrap_or_default();
                return Err(smoke(
                    viewport,
                    "strict visual",
                    format!("baseline differs at {diff_pixels}/{total_pixels} pixels{suffix}"),
                    &dir,
                ));
            }
            println!(
                "{SCENARIO}[{}]: non-strict baseline diff {diff_pixels}/{total_pixels} pixels",
                viewport.name
            );
        }
        Err(error) => {
            return Err(smoke(
                viewport,
                "baseline comparison",
                error.to_string(),
                &dir,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(render_path: &str) -> HybridCharacterSnapshot {
        HybridCharacterSnapshot {
            screen: "CharacterCreation".to_owned(),
            root_entity: "42v0".to_owned(),
            selected_part_ids: EXPECTED_STABLE_IDS
                .iter()
                .map(ToString::to_string)
                .collect(),
            part_count: EXPECTED_PART_COUNT,
            material_part_count: EXPECTED_MATERIAL_PART_COUNT,
            render_path: render_path.to_owned(),
        }
    }

    #[test]
    fn exact_identity_and_silhouette_are_required_on_every_screen() {
        let expected = snapshot("hybrid_material");
        assert!(
            snapshot_problem(
                &expected,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Creation,
                &[],
            )
            .is_none()
        );

        let mut wrong_id = expected.clone();
        wrong_id.selected_part_ids[2] = "human.hair.wrong.v1".to_owned();
        assert!(
            snapshot_problem(
                &wrong_id,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Creation,
                &[],
            )
            .is_some()
        );

        let mut wrong_count = expected;
        wrong_count.part_count -= 1;
        assert!(
            snapshot_problem(
                &wrong_count,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Creation,
                &[],
            )
            .is_some()
        );
    }

    #[test]
    fn stage_and_root_freshness_reject_stale_snapshots() {
        let creation = snapshot("hybrid_material");
        assert!(
            snapshot_problem(
                &creation,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Fight,
                &[],
            )
            .expect("creation telemetry is stale during fight")
            .contains("screen")
        );

        let mut later_fight = creation;
        later_fight.screen = "Fight".to_owned();
        assert!(
            snapshot_problem(
                &later_fight,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Fight,
                &["42v0"],
            )
            .expect("the first fight root is stale after shop")
            .contains("root")
        );
        later_fight.root_entity = "84v1".to_owned();
        assert!(
            snapshot_problem(
                &later_fight,
                ExpectedRenderPath::Hybrid,
                ExpectedStage::Fight,
                &["11v0", "42v0"],
            )
            .is_none()
        );
    }

    #[test]
    fn forced_fallback_requires_fallback_rendering_with_unchanged_facts() {
        let hybrid = snapshot("hybrid_material");
        let fallback = snapshot("albedo_fallback");

        assert!(
            snapshot_problem(
                &fallback,
                ExpectedRenderPath::Fallback,
                ExpectedStage::Creation,
                &[],
            )
            .is_none()
        );
        assert!(identity_or_silhouette_changed(&hybrid, &fallback).is_none());
        assert!(
            snapshot_problem(
                &hybrid,
                ExpectedRenderPath::Fallback,
                ExpectedStage::Creation,
                &[],
            )
            .is_some()
        );
    }

    #[test]
    fn fallback_console_filter_allows_only_missing_optional_channels() {
        let expected = vec![
            "log: %cERROR%c bevy_asset Path not found: assets/fighters/human/runtime/torso_mask.png color: red".to_owned(),
            "log: %cERROR%c bevy_asset Path not found: assets/fighters/human/runtime/head_normal.png color: red".to_owned(),
        ];
        assert!(unexpected_console_errors(&expected, ExpectedRenderPath::Fallback).is_empty());

        let mut unrelated = expected;
        unrelated.push("error: WebGL context lost".to_owned());
        unrelated.push("log: %cERROR%c combat invariant failed".to_owned());
        assert_eq!(
            unexpected_console_errors(&unrelated, ExpectedRenderPath::Fallback),
            vec![
                "error: WebGL context lost".to_owned(),
                "log: %cERROR%c combat invariant failed".to_owned(),
            ]
        );
    }

    #[test]
    fn optional_channel_filter_never_matches_albedos() {
        assert!(is_optional_material_channel("torso_mask.png"));
        assert!(is_optional_material_channel("torso_normal.png"));
        assert!(is_optional_material_channel("torso_shadow.png"));
        assert!(!is_optional_material_channel("torso.png"));
    }

    #[test]
    fn scenario_registers_exactly_two_visual_viewports() {
        assert_eq!(
            VIEWPORTS
                .iter()
                .map(|viewport| viewport.name)
                .collect::<Vec<_>>(),
            vec!["desktop", "phone"]
        );
    }
}
