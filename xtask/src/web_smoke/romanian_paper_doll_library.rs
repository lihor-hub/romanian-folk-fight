//! The `romanian-paper-doll-library` browser scenario (#323).
//!
//! One desktop and one phone session each drive all four Romanian presets through
//! creation, live combat, shop, a real page reload plus Continuă, and a fresh
//! combat. Four creator screenshots are the visual review surface; exact
//! reload-persistent ECS facts prove identity in the non-captured scenes and
//! prove the prepared Hoț de codru identity is the one on the live enemy rig.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus, ResourceEntry};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "romanian-paper-doll-library";

const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
const REVIEW_PAPER_DOLL_KEY: &str = "rff_review_paper_doll_v1";
const CAMPAIGN_SEED: u64 = 323;
const COMBAT_SEED: u64 = 20;
const HOT_DE_CODRU_SEED: u64 = 9_296_217_458_416_964_953;
const EXPECTED_PART_COUNT: usize = 15;
const MAX_FRAMES: usize = 3600;
const MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const STABLE_FRAMES_REQUIRED: usize = 3;
const MIN_CAPTURE_STABILITY: Duration = Duration::from_secs(10);

const HAIDUC_IDS: &[&str] = &[
    "human.body.zvelt.v1",
    "human.face.haiduc.v1",
    "human.hair.plete.v1",
    "human.torso.ie_altita.v1",
    "human.legs.itari.v1",
    "human.feet.opinci.v1",
];
const CIOBAN_IDS: &[&str] = &[
    "human.body.vanjos.v1",
    "human.face.cioban.v1",
    "human.hair.prins.v1",
    "human.torso.camasa_ciobaneasca.v1",
    "human.legs.cioareci.v1",
    "human.feet.opinci.v1",
];
const VOINIC_IDS: &[&str] = &[
    "human.body.voinic.v1",
    "human.face.voinic.v1",
    "human.hair.voinic_scurt.v1",
    "human.torso.camasa_voiniceasca.v1",
    "human.legs.cioareci_voinicesti.v1",
    "human.feet.opinci.v1",
];
const UCENIC_SOLOMONAR_IDS: &[&str] = &[
    "human.body.ucenic_solomonar.v1",
    "human.face.ucenic_solomonar.v1",
    "human.hair.ucenic_ciuf.v1",
    "human.torso.suman_de_ucenic.v1",
    "human.legs.cioareci_de_ucenic.v1",
    "human.feet.opinci.v1",
];
const HOT_DE_CODRU_IDS: &[&str] = &[
    "human.body.zvelt.v1",
    "human.face.cioban.v1",
    "human.hair.voinic_scurt.v1",
    "human.torso.camasa_ciobaneasca.v1",
    "human.legs.cioareci.v1",
    "human.feet.opinci.v1",
];

const VISUAL_CHECKPOINTS: [&str; 8] = [
    "desktop-haiduc",
    "desktop-voinic",
    "desktop-cioban",
    "desktop-ucenic-solomonar",
    "phone-haiduc",
    "phone-voinic",
    "phone-cioban",
    "phone-ucenic-solomonar",
];

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

#[derive(Debug, Clone, Copy)]
struct Look {
    key: &'static str,
    preset: &'static str,
    ids: &'static [&'static str],
    gear_part_count: usize,
    gear_asset_paths: &'static [&'static str],
    baseline_time: f32,
}

const LOOKS: &[Look] = &[
    Look {
        key: "haiduc",
        preset: "Haiducul",
        ids: HAIDUC_IDS,
        gear_part_count: 3,
        gear_asset_paths: &[
            "/assets/fighters/gear/runtime/topor_de_padurar.png",
            "/assets/fighters/gear/runtime/opinci_iuti.png",
        ],
        baseline_time: 10_000.0,
    },
    Look {
        key: "voinic",
        preset: "Voinicul",
        ids: VOINIC_IDS,
        gear_part_count: 2,
        gear_asset_paths: &[
            "/assets/fighters/gear/runtime/bata_ciobaneasca.png",
            "/assets/fighters/gear/runtime/scut_de_lemn.png",
        ],
        baseline_time: 15_000.0,
    },
    Look {
        key: "cioban",
        preset: "Ciobanul",
        ids: CIOBAN_IDS,
        gear_part_count: 3,
        gear_asset_paths: &[
            "/assets/fighters/gear/runtime/bata_ciobaneasca.png",
            "/assets/fighters/gear/runtime/caciula_de_oaie.png",
            "/assets/fighters/gear/runtime/cojoc_gros.png",
        ],
        baseline_time: 20_000.0,
    },
    Look {
        key: "ucenic-solomonar",
        preset: "Ucenicul Solomonar",
        ids: UCENIC_SOLOMONAR_IDS,
        gear_part_count: 3,
        gear_asset_paths: &[
            "/assets/fighters/gear/runtime/bata_ciobaneasca.png",
            "/assets/fighters/gear/runtime/ie_descantata.png",
            "/assets/fighters/gear/runtime/caciula_de_oaie.png",
        ],
        baseline_time: 25_000.0,
    },
];

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
struct IdentityFact {
    screen: String,
    root_entity: String,
    seed: Option<u64>,
    resolved_part_ids: Vec<String>,
    rig_source_ids: Vec<String>,
    part_count: usize,
    hybrid_part_count: usize,
    fallback_part_count: usize,
    #[serde(default)]
    gear_part_count: usize,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, PartialEq, Eq)]
struct PaperDollSnapshot {
    creation: Option<IdentityFact>,
    shop: Option<IdentityFact>,
    reloaded: Option<IdentityFact>,
    combat_player: Option<IdentityFact>,
    combat_npc: Option<IdentityFact>,
}

#[derive(Debug, Clone, Copy)]
enum FactSlot {
    Creation,
    Shop,
    Reloaded,
    CombatPlayer,
    CombatNpc,
}

impl FactSlot {
    fn get(self, snapshot: &PaperDollSnapshot) -> Option<&IdentityFact> {
        match self {
            Self::Creation => snapshot.creation.as_ref(),
            Self::Shop => snapshot.shop.as_ref(),
            Self::Reloaded => snapshot.reloaded.as_ref(),
            Self::CombatPlayer => snapshot.combat_player.as_ref(),
            Self::CombatNpc => snapshot.combat_npc.as_ref(),
        }
    }
}

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|error| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve review bundle"),
            error,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let mut missing_baseline = false;
    for viewport in VIEWPORTS {
        run_viewport(
            viewport,
            &server,
            update_baselines,
            strict_visual,
            &mut missing_baseline,
        )?;
    }
    let _ = artifacts::write_artifact(
        &artifacts::scenario_dir(SCENARIO),
        "server.log",
        server.request_log().join("\n"),
    );

    if update_baselines {
        println!(
            "\n{SCENARIO}: updated {} reviewed creator baselines under tests/visual/baselines/{SCENARIO}/.",
            VISUAL_CHECKPOINTS.len()
        );
    } else if missing_baseline {
        println!(
            "\n{SCENARIO}: semantic assertions passed, but an accepted creator baseline is missing."
        );
    } else {
        println!(
            "\n{SCENARIO}: desktop+phone visuals for all four Romanian presets and every cross-scene identity fact passed."
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
        .arg("dist-romanian-paper-doll-library");
    command.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (romanian-paper-doll-library)",
        command,
    )?;
    Ok(workspace_root().join("dist-romanian-paper-doll-library"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always lives directly under the workspace root")
        .to_path_buf()
}

fn run_viewport(
    viewport: &Viewport,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::scenario_dir(SCENARIO).join(viewport.name);
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);
    let checkpoint = browser::launch(viewport.width, viewport.height, 1.0, &profile_dir)
        .map_err(|error| smoke(viewport, "browser launch", error, &dir))?;
    checkpoint
        .navigate(&format!("{}/", server.base_url()))
        .map_err(|error| smoke(viewport, "navigation", error, &dir))?;
    wait_for_screen(&checkpoint, "MainMenu")
        .map_err(|error| smoke(viewport, "initial menu", error, &dir))?;

    for look in LOOKS {
        run_look(
            &checkpoint,
            viewport,
            look,
            update_baselines,
            strict_visual,
            missing_baseline,
        )?;
    }
    Ok(())
}

fn run_look(
    checkpoint: &Checkpoint,
    viewport: &Viewport,
    look: &Look,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let key = format!("{}-{}", viewport.name, look.key);
    let dir = artifacts::scenario_dir(SCENARIO).join(&key);
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "seedCombat", "seed": COMBAT_SEED}),
    )
    .map_err(|error| smoke(viewport, "seed combat", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
    )
    .map_err(|error| smoke(viewport, "start new game", error, &dir))?;
    wait_for_screen(checkpoint, "CharacterCreation")
        .map_err(|error| smoke(viewport, "creation screen", error, &dir))?;
    // NewGame resets every run-scoped resource, including CampaignSeed, so
    // the deterministic review seed must be applied after that production
    // reset and before confirmation prepares the encounter.
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "seedCampaign", "seed": CAMPAIGN_SEED}),
    )
    .map_err(|error| smoke(viewport, "seed campaign", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "selectPreset", "preset": look.preset}),
    )
    .map_err(|error| smoke(viewport, "select authored look", error, &dir))?;

    let creation = wait_for_fact(
        checkpoint,
        FactSlot::Creation,
        "CharacterCreation",
        look.ids,
        look.gear_part_count,
        None,
        &[],
    )
    .map_err(|error| smoke(viewport, "creation identity", error, &dir))?;
    wait_for_gear_assets(checkpoint, look.gear_asset_paths)
        .map_err(|error| smoke(viewport, "creator gear fetches", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "setTimeElapsed", "seconds": look.baseline_time}),
    )
    .map_err(|error| smoke(viewport, "set creator visual phase", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )
    .map_err(|error| smoke(viewport, "pause creator", error, &dir))?;
    let (status, screenshot) = capture_stable(checkpoint, viewport)
        .map_err(|error| smoke(viewport, "stable creator capture", error, &dir))?;
    assert_capture(viewport, &status, &screenshot)
        .map_err(|error| smoke(viewport, "creator capture assertions", error, &dir))?;
    write_checkpoint(&key, &status, &screenshot, &creation);
    handle_baseline(
        &key,
        viewport,
        &screenshot,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )
    .map_err(|error| smoke(viewport, "confirm hero", error, &dir))?;
    wait_for_screen(checkpoint, "Fight")
        .map_err(|error| smoke(viewport, "first combat", error, &dir))?;
    let first_combat = wait_for_fact(
        checkpoint,
        FactSlot::CombatPlayer,
        "Fight",
        look.ids,
        look.gear_part_count,
        None,
        &[],
    )
    .map_err(|error| smoke(viewport, "first combat player", error, &dir))?;
    let first_npc = wait_for_fact(
        checkpoint,
        FactSlot::CombatNpc,
        "Fight",
        HOT_DE_CODRU_IDS,
        0,
        Some(HOT_DE_CODRU_SEED),
        &[],
    )
    .map_err(|error| smoke(viewport, "first combat npc", error, &dir))?;
    require_same_identity(&creation, &first_combat)
        .map_err(|error| smoke(viewport, "creation to first combat", error, &dir))?;
    require_distinct_roots(&first_combat, &first_npc)
        .map_err(|error| smoke(viewport, "first combat roots", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )
    .map_err(|error| smoke(viewport, "unpause combat", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
    )
    .map_err(|error| smoke(viewport, "enable autoplay", error, &dir))?;
    wait_for_screen(checkpoint, "FightResult")
        .map_err(|error| smoke(viewport, "fight result", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
    )
    .map_err(|error| smoke(viewport, "go to shop", error, &dir))?;
    wait_for_screen(checkpoint, "Shop").map_err(|error| smoke(viewport, "shop", error, &dir))?;
    let shop = wait_for_fact(
        checkpoint,
        FactSlot::Shop,
        "Shop",
        look.ids,
        look.gear_part_count,
        None,
        &[],
    )
    .map_err(|error| smoke(viewport, "shop identity", error, &dir))?;
    require_same_identity(&creation, &shop)
        .map_err(|error| smoke(viewport, "creation to shop", error, &dir))?;

    checkpoint
        .reload()
        .map_err(|error| smoke(viewport, "page reload", error, &dir))?;
    wait_for_screen(checkpoint, "MainMenu")
        .map_err(|error| smoke(viewport, "menu after reload", error, &dir))?;
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "Continue"}),
    )
    .map_err(|error| smoke(viewport, "continue saved run", error, &dir))?;
    wait_for_screen(checkpoint, "Shop")
        .map_err(|error| smoke(viewport, "restored shop", error, &dir))?;
    let reloaded = wait_for_fact(
        checkpoint,
        FactSlot::Reloaded,
        "Shop",
        look.ids,
        look.gear_part_count,
        None,
        &[&shop.root_entity],
    )
    .map_err(|error| smoke(viewport, "reloaded identity", error, &dir))?;
    require_same_identity(&creation, &reloaded)
        .map_err(|error| smoke(viewport, "creation to reload", error, &dir))?;

    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "BackToArena"}),
    )
    .map_err(|error| smoke(viewport, "leave restored shop", error, &dir))?;
    wait_for_screen(checkpoint, "Fight")
        .map_err(|error| smoke(viewport, "restored combat", error, &dir))?;
    let combat_player = wait_for_fact(
        checkpoint,
        FactSlot::CombatPlayer,
        "Fight",
        look.ids,
        look.gear_part_count,
        None,
        &[&first_combat.root_entity],
    )
    .map_err(|error| smoke(viewport, "restored combat player", error, &dir))?;
    require_same_identity(&creation, &combat_player)
        .map_err(|error| smoke(viewport, "creation to restored combat", error, &dir))?;
    if creation.resolved_part_ids == first_npc.resolved_part_ids {
        return Err(smoke(
            viewport,
            "player/npc distinction",
            "seeded Hoț de codru unexpectedly reused the player identity".to_owned(),
            &dir,
        ));
    }

    let final_snapshot = read_snapshot(checkpoint)
        .map_err(|error| smoke(viewport, "final telemetry", error, &dir))?
        .ok_or_else(|| {
            smoke(
                viewport,
                "final telemetry",
                "snapshot disappeared".to_owned(),
                &dir,
            )
        })?;
    let retained_npc = final_snapshot.combat_npc.as_ref().ok_or_else(|| {
        smoke(
            viewport,
            "final npc telemetry",
            "seeded Hoț fact disappeared after the later Strigoi arena".to_owned(),
            &dir,
        )
    })?;
    require_same_identity(&first_npc, retained_npc)
        .map_err(|error| smoke(viewport, "retained prepared/live npc parity", error, &dir))?;
    let _ = artifacts::write_artifact(
        &dir,
        "cross-scene-snapshot.json",
        serde_json::to_string_pretty(&final_snapshot).unwrap_or_default(),
    );
    let final_status = checkpoint
        .read_status()
        .map_err(|error| smoke(viewport, "final page status", error, &dir))?;
    check_no_errors(&final_status)
        .map_err(|error| smoke(viewport, "final console", error, &dir))?;

    // Return through the production pause/abandon flow. It clears the run
    // save and lands on MainMenu, ready for the second authored look.
    checkpoint
        .press_key("Escape")
        .map_err(|error| smoke(viewport, "open pause overlay", error, &dir))?;
    for _ in 0..12 {
        checkpoint
            .wait_for_frame()
            .map_err(|error| smoke(viewport, "settle pause overlay", error, &dir))?;
    }
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "PauseAbandon"}),
    )
    .map_err(|error| smoke(viewport, "abandon completed run", error, &dir))?;
    wait_for_screen(checkpoint, "MainMenu")
        .map_err(|error| smoke(viewport, "menu before next look", error, &dir))?;

    println!(
        "{SCENARIO}[{key}]: exact creation/shop/reload/combat identity, 15 source IDs, hybrid={}, fallback=0, NPC seed={HOT_DE_CODRU_SEED}",
        combat_player.hybrid_part_count
    );
    Ok(())
}

fn expected_rig_source_ids(ids: &[&str]) -> Vec<String> {
    let body = ids[0];
    let face = ids[1];
    let hair = ids[2];
    let torso = ids[3];
    let legs = ids[4];
    let feet = ids[5];
    [
        body, body, body, legs, legs, feet, torso, hair, face, body, body, body, legs, legs, feet,
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

fn fact_problem_with_seed(
    fact: &IdentityFact,
    screen: &str,
    ids: &[&str],
    gear_part_count: usize,
    seed: Option<u64>,
    rejected_roots: &[&str],
) -> Option<String> {
    let expected_ids = ids.iter().map(ToString::to_string).collect::<Vec<_>>();
    if fact.screen != screen {
        return Some(format!("screen {:?}, expected {screen:?}", fact.screen));
    }
    if fact.root_entity.is_empty() || rejected_roots.contains(&fact.root_entity.as_str()) {
        return Some(format!("root {:?} is empty or stale", fact.root_entity));
    }
    if fact.seed != seed {
        return Some(format!("seed {:?}, expected {seed:?}", fact.seed));
    }
    if fact.resolved_part_ids != expected_ids {
        return Some(format!(
            "resolved IDs {:?}, expected {expected_ids:?}",
            fact.resolved_part_ids
        ));
    }
    let expected_sources = expected_rig_source_ids(ids);
    if fact.rig_source_ids != expected_sources {
        return Some(format!(
            "rig source IDs {:?}, expected {expected_sources:?}",
            fact.rig_source_ids
        ));
    }
    if fact.part_count != EXPECTED_PART_COUNT || fact.rig_source_ids.len() != EXPECTED_PART_COUNT {
        return Some(format!(
            "part/source count {}/{}, expected {EXPECTED_PART_COUNT}",
            fact.part_count,
            fact.rig_source_ids.len()
        ));
    }
    if fact.hybrid_part_count == 0 {
        return Some("no rig part uses the hybrid material".to_owned());
    }
    if fact.fallback_part_count != 0 {
        return Some(format!(
            "{} rig parts remain on fallback",
            fact.fallback_part_count
        ));
    }
    if fact.gear_part_count != gear_part_count {
        return Some(format!(
            "{} gear layers, expected {gear_part_count}",
            fact.gear_part_count
        ));
    }
    None
}

fn identity_changed(expected: &IdentityFact, actual: &IdentityFact) -> Option<String> {
    (expected.seed != actual.seed
        || expected.resolved_part_ids != actual.resolved_part_ids
        || expected.rig_source_ids != actual.rig_source_ids
        || expected.part_count != actual.part_count
        || expected.gear_part_count != actual.gear_part_count)
        .then(|| format!("identity changed: expected={expected:?}, actual={actual:?}"))
}

fn require_same_identity(expected: &IdentityFact, actual: &IdentityFact) -> Result<(), String> {
    identity_changed(expected, actual).map_or(Ok(()), Err)
}

fn require_distinct_roots(left: &IdentityFact, right: &IdentityFact) -> Result<(), String> {
    (left.root_entity != right.root_entity)
        .then_some(())
        .ok_or_else(|| {
            format!(
                "two live rigs share root marker {:?}: left={left:?}, right={right:?}",
                left.root_entity
            )
        })
}

fn wait_for_fact(
    checkpoint: &Checkpoint,
    slot: FactSlot,
    screen: &str,
    ids: &[&str],
    gear_part_count: usize,
    seed: Option<u64>,
    rejected_roots: &[&str],
) -> Result<IdentityFact, String> {
    let start = Instant::now();
    let mut observed_frames = 0;
    let mut last = None;
    while !readiness_poll_exhausted(start.elapsed(), observed_frames) {
        checkpoint.wait_for_frame()?;
        observed_frames += 1;
        if let Some(snapshot) = read_snapshot(checkpoint)?
            && let Some(fact) = slot.get(&snapshot)
        {
            if fact_problem_with_seed(fact, screen, ids, gear_part_count, seed, rejected_roots)
                .is_none()
            {
                return Ok(fact.clone());
            }
            last = Some(fact.clone());
        }
        std::thread::sleep(READINESS_POLL_INTERVAL);
    }
    Err(format!(
        "never observed exact {slot:?} fact for {screen} with a fresh root within \
         {MAX_WALL_CLOCK:?}; observed_frames={observed_frames}, last={last:?}"
    ))
}

fn readiness_poll_exhausted(elapsed: Duration, _observed_frames: usize) -> bool {
    elapsed > MAX_WALL_CLOCK
}

fn wait_for_gear_assets(checkpoint: &Checkpoint, paths: &[&str]) -> Result<(), String> {
    let start = Instant::now();
    let mut last_resources = Vec::new();
    while start.elapsed() <= MAX_WALL_CLOCK {
        checkpoint.wait_for_frame()?;
        let status = checkpoint.read_status()?;
        if gear_assets_fetched(&status.resources, paths) {
            return Ok(());
        }
        last_resources = status
            .resources
            .into_iter()
            .filter(|resource| paths.iter().any(|path| resource.url.ends_with(path)))
            .collect();
        std::thread::sleep(READINESS_POLL_INTERVAL);
    }
    Err(format!(
        "gear fetches did not complete within {MAX_WALL_CLOCK:?}; expected={paths:?}, \
         observed={last_resources:?}"
    ))
}

fn gear_assets_fetched(resources: &[ResourceEntry], paths: &[&str]) -> bool {
    paths.iter().all(|path| {
        resources.iter().any(|resource| {
            resource.url.ends_with(path)
                && (200..300).contains(&resource.status)
                && resource.transfer_size > 0.0
        })
    })
}

fn read_snapshot(checkpoint: &Checkpoint) -> Result<Option<PaperDollSnapshot>, String> {
    let Some(json) =
        checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_PAPER_DOLL_KEY}')"))?
    else {
        return Ok(None);
    };
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|error| format!("invalid paper-doll snapshot {json}: {error}"))
}

fn send_command(checkpoint: &Checkpoint, payload: serde_json::Value) -> Result<(), String> {
    let json = payload.to_string();
    let literal = serde_json::to_string(&json).map_err(|error| error.to_string())?;
    checkpoint.eval_unit(&format!(
        "localStorage.setItem('{REVIEW_COMMAND_KEY}', {literal});"
    ))?;
    for _ in 0..300 {
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

fn capture_stable(
    checkpoint: &Checkpoint,
    viewport: &Viewport,
) -> Result<(PageStatus, Vec<u8>), String> {
    let mut stable_since = Instant::now();
    let mut previous = None;
    let mut stable = 0usize;
    for _ in 0..300 {
        checkpoint.wait_for_frame()?;
        let screenshot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
        if previous.as_deref() == Some(screenshot.as_slice()) {
            stable += 1;
        } else {
            stable = 1;
            stable_since = Instant::now();
        }
        previous = Some(screenshot);
        if capture_stability_reached(stable_since.elapsed(), stable) {
            return Ok((
                checkpoint.read_status()?,
                previous.expect("the stable frame was just captured"),
            ));
        }
    }
    Err("creator screenshot did not stabilize after pausing virtual time".to_owned())
}

fn capture_stability_reached(elapsed: Duration, stable_frames: usize) -> bool {
    // Virtual time is paused, but texture fetch/decode and render extraction
    // continue on real time. Requiring a real-time stability window prevents
    // two early identical frames from winning just before a gear PNG appears.
    elapsed >= MIN_CAPTURE_STABILITY && stable_frames >= STABLE_FRAMES_REQUIRED
}

fn check_no_errors(status: &PageStatus) -> Result<(), String> {
    if !status.errors.is_empty() {
        return Err(format!("page errors: {:?}", status.errors));
    }
    let console_errors = status
        .console
        .iter()
        .filter(|line| line.starts_with("error:") || line.contains("%cERROR%c"))
        .collect::<Vec<_>>();
    if !console_errors.is_empty() {
        return Err(format!("console errors: {console_errors:?}"));
    }
    Ok(())
}

fn assert_capture(
    viewport: &Viewport,
    status: &PageStatus,
    screenshot: &[u8],
) -> Result<(), String> {
    check_no_errors(status)?;
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

fn write_checkpoint(key: &str, status: &PageStatus, screenshot: &[u8], fact: &IdentityFact) {
    let Ok(dir) = artifacts::checkpoint_dir(SCENARIO, key) else {
        return;
    };
    let _ = artifacts::write_artifact(&dir, "screenshot.png", screenshot);
    let _ = artifacts::write_artifact(&dir, "console.log", status.console.join("\n"));
    let _ = artifacts::write_artifact(
        &dir,
        "creation-fact.json",
        serde_json::to_string_pretty(fact).unwrap_or_default(),
    );
}

fn handle_baseline(
    key: &str,
    viewport: &Viewport,
    screenshot: &[u8],
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::scenario_dir(SCENARIO).join(key);
    match baseline::handle(SCENARIO, key, screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => println!(
            "{SCENARIO}[{key}]: baseline updated at {}",
            baseline::baseline_path(SCENARIO, key).display()
        ),
        Ok(baseline::BaselineOutcome::Matches) => {
            println!("{SCENARIO}[{key}]: matches accepted baseline")
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            if strict_visual {
                return Err(smoke(
                    viewport,
                    "strict visual",
                    format!("accepted baseline {key:?} is missing"),
                    &dir,
                ));
            }
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff = baseline::write_diff_triplet(SCENARIO, key, screenshot, &dir);
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
                "{SCENARIO}[{key}]: non-strict baseline diff {diff_pixels}/{total_pixels} pixels"
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

fn smoke(viewport: &Viewport, step: &str, error: String, dir: &Path) -> SmokeError {
    SmokeError::scenario(
        format!("web-smoke {SCENARIO}[{}]: {step}", viewport.name),
        error,
        dir.to_path_buf(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(screen: &str, root: &str) -> IdentityFact {
        IdentityFact {
            screen: screen.to_owned(),
            root_entity: root.to_owned(),
            seed: None,
            resolved_part_ids: HAIDUC_IDS.iter().map(ToString::to_string).collect(),
            rig_source_ids: expected_rig_source_ids(HAIDUC_IDS),
            part_count: EXPECTED_PART_COUNT,
            hybrid_part_count: 15,
            fallback_part_count: 0,
            gear_part_count: 3,
        }
    }

    #[test]
    fn every_fact_requires_exact_identity_materials_and_a_fresh_scene_root() {
        let creation = fact("CharacterCreation", "10v0");
        assert!(
            fact_problem_with_seed(&creation, "CharacterCreation", HAIDUC_IDS, 3, None, &[])
                .is_none()
        );

        let reused_on_another_screen = fact("Shop", "10v0");
        assert!(
            fact_problem_with_seed(&reused_on_another_screen, "Shop", HAIDUC_IDS, 3, None, &[])
                .is_none(),
            "screen plus root is the freshness marker; Bevy may reuse an entity id"
        );
        assert!(
            fact_problem_with_seed(
                &reused_on_another_screen,
                "Shop",
                HAIDUC_IDS,
                3,
                None,
                &["10v0"],
            )
            .is_some(),
            "the same-screen prior root is stale"
        );

        let mut fallback = fact("Shop", "20v0");
        fallback.fallback_part_count = 1;
        assert!(
            fact_problem_with_seed(&fallback, "Shop", HAIDUC_IDS, 3, None, &["10v0"]).is_some()
        );

        let mut missing_gear = fact("CharacterCreation", "30v0");
        missing_gear.gear_part_count = 2;
        assert!(
            fact_problem_with_seed(&missing_gear, "CharacterCreation", HAIDUC_IDS, 3, None, &[],)
                .is_some(),
            "a baseline cannot capture before every equipment layer is spawned"
        );
    }

    #[test]
    fn cross_scene_player_identity_and_seeded_npc_identity_are_compared_exactly() {
        let creation = fact("CharacterCreation", "10v0");
        let shop = fact("Shop", "20v0");
        let reloaded = fact("Shop", "30v1");
        let combat = fact("Fight", "40v0");
        assert!(identity_changed(&creation, &shop).is_none());
        assert!(identity_changed(&creation, &reloaded).is_none());
        assert!(identity_changed(&creation, &combat).is_none());

        let mut npc = fact("Fight", "41v0");
        npc.seed = Some(HOT_DE_CODRU_SEED);
        npc.resolved_part_ids[0] = "human.body.other.v1".to_owned();
        assert!(identity_changed(&creation, &npc).is_some());
    }

    #[test]
    fn scenario_owns_eight_creator_visual_checkpoints() {
        assert_eq!(
            VISUAL_CHECKPOINTS,
            [
                "desktop-haiduc",
                "desktop-voinic",
                "desktop-cioban",
                "desktop-ucenic-solomonar",
                "phone-haiduc",
                "phone-voinic",
                "phone-cioban",
                "phone-ucenic-solomonar",
            ]
        );
    }

    #[test]
    fn material_readiness_budget_is_wall_clock_based_not_frame_count_based() {
        assert!(
            !readiness_poll_exhausted(Duration::from_secs(1), MAX_FRAMES + 1),
            "rapid frame acknowledgements must not exhaust time still available for asset I/O"
        );
        assert!(readiness_poll_exhausted(
            MAX_WALL_CLOCK + Duration::from_millis(1),
            1,
        ));
        assert!(READINESS_POLL_INTERVAL > Duration::ZERO);
    }

    #[test]
    fn creator_capture_requires_a_real_time_stability_window() {
        assert!(!capture_stability_reached(
            MIN_CAPTURE_STABILITY - Duration::from_millis(1),
            STABLE_FRAMES_REQUIRED + 1,
        ));
        assert!(!capture_stability_reached(
            MIN_CAPTURE_STABILITY,
            STABLE_FRAMES_REQUIRED - 1,
        ));
        assert!(capture_stability_reached(
            MIN_CAPTURE_STABILITY,
            STABLE_FRAMES_REQUIRED,
        ));
    }

    #[test]
    fn creator_capture_requires_every_gear_fetch_to_complete() {
        let paths = ["/assets/a.png", "/assets/b.png"];
        let mut resources = vec![ResourceEntry {
            url: "http://127.0.0.1/assets/a.png".to_owned(),
            status: 200,
            transfer_size: 42.0,
        }];
        assert!(!gear_assets_fetched(&resources, &paths));

        resources.push(ResourceEntry {
            url: "http://127.0.0.1/assets/b.png".to_owned(),
            status: 200,
            transfer_size: 0.0,
        });
        assert!(!gear_assets_fetched(&resources, &paths));

        resources[1].transfer_size = 7.0;
        assert!(gear_assets_fetched(&resources, &paths));
    }
}
