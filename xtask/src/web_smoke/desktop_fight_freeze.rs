//! Deterministic virtual-time freeze for desktop fight screenshots.
//!
//! Pausing first closes the race between choosing an absolute animation
//! phase and the next browser frame. The review seam can then advance
//! `Time<Virtual>` to one fixed elapsed target while it remains paused.
//! Two exact motion-telemetry samples prove that the arena observed the
//! command and held still before a screenshot is accepted.

use crate::web_smoke::browser::Checkpoint;

const BASELINE_TIME_SECONDS: f32 = 10_000.0;
const REVIEW_MOTION_KEY: &str = "rff_review_motion_v1";
const MOTION_MAX_FRAMES: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq)]
enum FreezeStep {
    Pause,
    SetElapsed(f32),
    EnterFight,
    AssertMotion,
}

fn freeze_steps() -> [FreezeStep; 4] {
    [
        FreezeStep::Pause,
        FreezeStep::SetElapsed(BASELINE_TIME_SECONDS),
        FreezeStep::EnterFight,
        FreezeStep::AssertMotion,
    ]
}

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
struct ParallaxSample {
    base_x: f64,
    x: f64,
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct MotionSnapshot {
    player_x: f64,
    player_staged_x: f64,
    enemy_x: f64,
    enemy_staged_x: f64,
    camera_x: f64,
    camera_y: f64,
    parallax: Vec<ParallaxSample>,
    /// Preserve every current or future telemetry field so the equality
    /// assertion covers the complete published payload, not only the
    /// coordinates this helper understands by name.
    #[serde(flatten)]
    additional: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Freezes one desktop fight at an absolute virtual-time phase, then proves
/// through the arena's exact review telemetry that consecutive rendered
/// frames remain at the same motion state.
pub(crate) fn freeze(
    checkpoint: &Checkpoint,
    mut send_command: impl FnMut(serde_json::Value) -> Result<(), String>,
    mut enter_fight: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    for step in freeze_steps() {
        match step {
            FreezeStep::Pause => {
                send_command(serde_json::json!({"cmd": "setTimePaused", "paused": true}))?
            }
            FreezeStep::SetElapsed(seconds) => {
                send_command(serde_json::json!({"cmd": "setTimeElapsed", "seconds": seconds}))?
            }
            FreezeStep::EnterFight => enter_fight()?,
            FreezeStep::AssertMotion => assert_motion_frozen_in_browser(checkpoint)?,
        }
    }
    Ok(())
}

fn assert_motion_frozen_in_browser(checkpoint: &Checkpoint) -> Result<(), String> {
    let first = wait_for_motion(checkpoint)?;
    checkpoint.wait_for_frame()?;
    let second = read_motion(checkpoint)?;
    assert_frozen_motion(Some(&first), second.as_ref())
}

fn wait_for_motion(checkpoint: &Checkpoint) -> Result<MotionSnapshot, String> {
    for _ in 0..MOTION_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_motion(checkpoint)? {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "never observed desktop fight motion telemetry under {REVIEW_MOTION_KEY:?} across \
         {MOTION_MAX_FRAMES} frames after freezing virtual time"
    ))
}

fn read_motion(checkpoint: &Checkpoint) -> Result<Option<MotionSnapshot>, String> {
    let raw = checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_MOTION_KEY}')"))?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|error| format!("motion snapshot was not valid JSON ({json}): {error}")),
    }
}

fn assert_frozen_motion(
    first: Option<&MotionSnapshot>,
    second: Option<&MotionSnapshot>,
) -> Result<(), String> {
    let first = first.ok_or_else(|| {
        format!("first desktop fight motion sample was missing from {REVIEW_MOTION_KEY:?}")
    })?;
    let second = second.ok_or_else(|| {
        format!("second desktop fight motion sample was missing from {REVIEW_MOTION_KEY:?}")
    })?;
    if first != second {
        return Err(format!(
            "desktop fight motion changed after the absolute-time freeze: \
             first={first:?}, second={second:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_before_setting_the_absolute_fight_phase() {
        assert_eq!(freeze_steps()[0], FreezeStep::Pause);
        assert_eq!(
            freeze_steps()[1],
            FreezeStep::SetElapsed(BASELINE_TIME_SECONDS)
        );
    }

    #[test]
    fn freezes_the_clock_before_entering_the_fight() {
        assert_eq!(
            freeze_steps(),
            [
                FreezeStep::Pause,
                FreezeStep::SetElapsed(BASELINE_TIME_SECONDS),
                FreezeStep::EnterFight,
                FreezeStep::AssertMotion,
            ]
        );
    }

    fn motion_fixture() -> MotionSnapshot {
        MotionSnapshot {
            player_x: -180.0,
            player_staged_x: -180.0,
            enemy_x: 180.0,
            enemy_staged_x: 180.0,
            camera_x: 0.0,
            camera_y: 0.0,
            parallax: vec![ParallaxSample {
                base_x: 0.0,
                x: 12.5,
            }],
            additional: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn accepts_two_identical_motion_samples() {
        let first = motion_fixture();
        let second = first.clone();

        assert_eq!(assert_frozen_motion(Some(&first), Some(&second)), Ok(()));
    }

    #[test]
    fn rejects_motion_that_changes_after_the_freeze() {
        let first = motion_fixture();
        let mut second = first.clone();
        second.parallax[0].x += 1.0;

        let error = assert_frozen_motion(Some(&first), Some(&second)).unwrap_err();
        assert!(error.contains("motion changed after the absolute-time freeze"));
    }

    #[test]
    fn rejects_changes_in_additional_motion_fields() {
        let motion = |opponent: &str| {
            serde_json::from_value::<MotionSnapshot>(serde_json::json!({
                "player_x": -180.0,
                "player_staged_x": -180.0,
                "enemy_x": 180.0,
                "enemy_staged_x": 180.0,
                "camera_x": 0.0,
                "camera_y": 0.0,
                "parallax": [{"base_x": 0.0, "x": 12.5}],
                "generated_opponent": {"name": opponent},
            }))
            .unwrap()
        };
        let first = motion("A");
        let second = motion("B");

        let error = assert_frozen_motion(Some(&first), Some(&second)).unwrap_err();
        assert!(error.contains("motion changed after the absolute-time freeze"));
    }

    #[test]
    fn rejects_missing_motion_telemetry() {
        let error = assert_frozen_motion(None, None).unwrap_err();
        assert!(error.contains(REVIEW_MOTION_KEY));
    }
}
