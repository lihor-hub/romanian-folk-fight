//! Sprite animation for the arena fighters (#22): the [`SpriteAnimation`]
//! timer component driving a `TextureAtlas` index, the per-fighter clip
//! table ([`FighterClip`]), the sprite-sheet handles and readiness guard
//! ([`FighterSpriteSheets`]), and the wiring that turns [`CombatLogEvent`]s
//! into attack / hurt / KO / footwork animations. Movement events update
//! the persistent [`ArenaStaging`] positions and tween the fighters to
//! their new staged x — fighters never return to fixed anchors — while the
//! attack lunge arcs out from and back to the attacker's current staged x.

use std::time::Duration;

use bevy::prelude::*;

use crate::character::{EnemyFighter, PlayerFighter};
use crate::combat::{CombatAction, CombatEvent, CombatLogEvent, CombatSide};
use crate::cutout::{
    CutoutPartMarker, CutoutPartRestPose, CutoutPose, CutoutRig, cutout_rig_owner,
};
use crate::roster::LADDER;
use crate::settings::AccessibilityPreferences;

use super::ArenaStaging;

/// Side length of one sprite-sheet frame in pixels.
pub const FRAME_SIZE: u32 = 128;
/// Frames per sheet row; the sheets are a [`ATLAS_COLUMNS`] x [`ATLAS_ROWS`]
/// grid in row-major order (idle row, attack row, hurt + KO row, footwork
/// row).
pub const ATLAS_COLUMNS: u32 = 4;
/// Rows per sprite sheet.
pub const ATLAS_ROWS: u32 = 4;

/// The player's sprite sheet.
const PLAYER_SHEET: &str = "sprites/player.png";

/// One sheet per roster rung, indexed like [`LADDER`]. All sheets share the
/// same frame layout, so one atlas layout serves every fighter.
const OPPONENT_SHEETS: [&str; 10] = [
    "sprites/hot_de_codru.png",
    "sprites/strigoi.png",
    "sprites/varcolac.png",
    "sprites/capcaun.png",
    "sprites/muma_padurii.png",
    "sprites/iele.png",
    "sprites/solomonar.png",
    "sprites/balaur.png",
    "sprites/zmeu.png",
    "sprites/zmeul_zmeilor.png",
];

/// How far towards the opponent the attack lunge peaks, as a fraction of
/// the current staged gap between the two fighters.
const LUNGE_FRACTION: f32 = 0.35;

/// Reduced-motion (#200) displacement for the attack lunge, in world units
/// (the codebase treats one world unit as one logical pixel for
/// presentation-scale distances -- see `fx::DRIFT_AMPLITUDE`'s doc comment
/// for the same convention). Chosen as the documented safe treatment: a
/// small, barely-there nudge (well under the issue's "≤8px" ceiling) that
/// still gives the strike a readable beat instead of removing displacement
/// entirely, while never approaching the full lunge (up to ~126 world
/// units at the far band). Applies with the exact same easing curve and
/// timer duration as full motion -- only the peak distance shrinks.
/// Footwork is different: a staged position change is semantic state, not
/// decoration, so reduced motion replaces its tween with a near-instant
/// snap to the new x instead of shrinking it (see [`footwork_x`]).
const REDUCED_MOTION_DISPLACEMENT: f32 = 6.0;

/// Duration of a footwork tween from the old staged x to the new one
/// (ease-out, see [`footwork_x`]).
const FOOTWORK_DURATION: Duration = Duration::from_millis(450);

/// Short readable hold for non-sheet rig-only defensive reactions.
const RIG_REACTION_DURATION: Duration = Duration::from_millis(360);

/// How a [`SpriteAnimation`] behaves at its last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationMode {
    /// Wrap back to the first frame forever (idle).
    Loop,
    /// Show the last frame for one frame-duration, then report finished
    /// (attack, hurt, KO). What happens next is up to the clip: everything
    /// returns to idle except KO, which freezes on its final frame.
    Once,
}

/// Timer-driven frame range over a `TextureAtlas`: advances the atlas index
/// from `first` to `last` at `fps`, then loops or finishes per `mode`.
#[derive(Component, Debug, Clone)]
pub struct SpriteAnimation {
    /// First atlas index of the clip.
    pub first: usize,
    /// Last atlas index of the clip (inclusive).
    pub last: usize,
    /// Frames per second the clip plays at.
    pub fps: f32,
    /// Loop or play once.
    pub mode: AnimationMode,
    current: usize,
    timer: Timer,
    finished: bool,
}

impl SpriteAnimation {
    /// A fresh animation positioned at `first`.
    pub fn new(first: usize, last: usize, fps: f32, mode: AnimationMode) -> Self {
        Self {
            first,
            last,
            fps,
            mode,
            current: first,
            timer: Timer::new(Duration::from_secs_f32(1.0 / fps), TimerMode::Repeating),
            finished: false,
        }
    }

    /// Advances the clip by `delta` from the frame `current`, returning the
    /// new atlas index when it changes. A [`AnimationMode::Once`] clip that
    /// completes its last frame flips to [`Self::is_finished`] and never
    /// moves again.
    pub fn advance(&mut self, delta: Duration, current: usize) -> Option<usize> {
        if self.finished {
            return None;
        }
        self.timer.tick(delta);
        let mut frame = current.clamp(self.first, self.last);
        let mut moved = false;
        for _ in 0..self.timer.times_finished_this_tick() {
            if frame < self.last {
                frame += 1;
                moved = true;
            } else {
                match self.mode {
                    AnimationMode::Loop => {
                        frame = self.first;
                        moved = true;
                    }
                    AnimationMode::Once => {
                        self.finished = true;
                        break;
                    }
                }
            }
        }
        self.current = frame;
        moved.then_some(frame)
    }

    /// Current frame for cutout-root animations that do not have a root atlas
    /// sprite to store the index.
    pub fn current_frame(&self) -> usize {
        self.current
    }

    /// Snaps the current frame inside this clip without ticking the timer.
    pub fn set_current_frame(&mut self, frame: usize) {
        self.current = frame.clamp(self.first, self.last);
    }

    /// Whether a [`AnimationMode::Once`] clip has completed its last frame.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Total play time of one pass over the clip.
    pub fn clip_duration(&self) -> Duration {
        Duration::from_secs_f32((self.last - self.first + 1) as f32 / self.fps)
    }
}

/// The fighter clips, mapped onto the shared sheet layout. Doubles as
/// the component tracking which clip a fighter is currently playing.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FighterClip {
    Idle,
    Attack,
    Hurt,
    Ko,
    StepForward,
    StepBack,
}

impl FighterClip {
    /// The animation for this clip on the shared 4x4 sheet layout.
    pub fn animation(self) -> SpriteAnimation {
        match self {
            Self::Idle => SpriteAnimation::new(0, 3, 6.0, AnimationMode::Loop),
            Self::Attack => SpriteAnimation::new(4, 7, 10.0, AnimationMode::Once),
            Self::Hurt => SpriteAnimation::new(8, 9, 8.0, AnimationMode::Once),
            Self::Ko => SpriteAnimation::new(10, 11, 6.0, AnimationMode::Once),
            Self::StepForward => SpriteAnimation::new(12, 13, 4.0, AnimationMode::Once),
            Self::StepBack => SpriteAnimation::new(14, 15, 4.0, AnimationMode::Once),
        }
    }
}

/// Handles to every fighter sprite sheet plus the shared atlas layout.
/// Loaded at startup; [`FighterSpriteSheets::ready`] is the loading guard
/// the arena waits on before spawning fighters (and thus before combat can
/// begin). In headless apps without an `AssetServer` the default handles are
/// used and everything counts as ready.
#[derive(Resource, Debug, Clone, Default)]
pub struct FighterSpriteSheets {
    /// The shared 4x3 grid layout of every sheet.
    pub layout: Handle<TextureAtlasLayout>,
    /// The player's sheet.
    pub player: Handle<Image>,
    /// One sheet per [`LADDER`] rung.
    pub opponents: [Handle<Image>; 10],
}

impl FighterSpriteSheets {
    /// The sheet of the roster opponent at `ladder_index` (pre-wrap).
    pub fn opponent(&self, ladder_index: usize) -> Handle<Image> {
        self.opponents[ladder_index % LADDER.len()].clone()
    }

    /// Whether every sheet is fully loaded. Without an asset server (tests)
    /// there is nothing to wait for.
    pub fn ready(&self, asset_server: Option<&AssetServer>) -> bool {
        let Some(server) = asset_server else {
            return true;
        };
        std::iter::once(&self.player)
            .chain(self.opponents.iter())
            .all(|sheet| server.is_loaded_with_dependencies(sheet))
    }
}

/// Kicks off the sprite-sheet loads at startup so the sheets are usually
/// ready long before the first fight. Headless apps (no `AssetServer`) get
/// default handles, which [`FighterSpriteSheets::ready`] treats as loaded.
fn load_fighter_sheets(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    layouts: Option<ResMut<Assets<TextureAtlasLayout>>>,
) {
    let mut sheets = FighterSpriteSheets::default();
    if let (Some(server), Some(mut layouts)) = (asset_server, layouts) {
        sheets.layout = layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(FRAME_SIZE),
            ATLAS_COLUMNS,
            ATLAS_ROWS,
            None,
            None,
        ));
        sheets.player = server.load(PLAYER_SHEET);
        sheets.opponents = OPPONENT_SHEETS.map(|path| server.load(path));
    }
    commands.insert_resource(sheets);
}

/// Attack lunge of one fighter: an out-and-back arc from the fighter's
/// *current* staged x towards the opponent's, lasting exactly one attack
/// clip. Only the side is stored — the endpoints are read live from
/// [`ArenaStaging`] every frame, so a mid-lunge pair slide (see
/// [`ArenaStaging::apply_move`]) never strands the fighter on a stale
/// return position.
#[derive(Component, Debug, Clone)]
pub struct AttackLunge {
    side: CombatSide,
    timer: Timer,
}

impl AttackLunge {
    /// A lunge for `side`, timed to the attack clip.
    fn for_side(side: CombatSide) -> Self {
        Self {
            side,
            timer: Timer::new(
                FighterClip::Attack.animation().clip_duration(),
                TimerMode::Once,
            ),
        }
    }
}

/// X of a lunging fighter at `progress` in `0..=1`: an out-and-back arc
/// from the fighter's staged `from_x` peaking at [`LUNGE_FRACTION`] of the
/// current gap towards the opponent's staged `toward_x` (or, under reduced
/// motion, at [`REDUCED_MOTION_DISPLACEMENT`] in the same direction -- see
/// that constant's docs). Recomputed from its inputs alone every call, with
/// no state of its own, so a caller re-evaluating this mid-lunge with a
/// flipped `reduced_motion` gets a consistent position for the new mode
/// immediately -- nothing to restore separately.
pub fn lunge_x(from_x: f32, toward_x: f32, progress: f32, reduced_motion: bool) -> f32 {
    // `sin(PI)` is a hair negative in f32; the clamp keeps the endpoints
    // exactly on the staged x.
    let arc = (progress.clamp(0.0, 1.0) * std::f32::consts::PI)
        .sin()
        .max(0.0);
    let full_delta = (toward_x - from_x) * LUNGE_FRACTION;
    let delta = if reduced_motion {
        full_delta.signum() * REDUCED_MOTION_DISPLACEMENT
    } else {
        full_delta
    };
    from_x + delta * arc
}

/// One fighter's movement tween from its old staged x to its new one.
/// The staged positions themselves are combat truth (see [`ArenaStaging`]);
/// this component only paces the transition — the fighter always ends at
/// `to_x` and never returns to where it started.
#[derive(Component, Debug, Clone)]
struct FootworkStep {
    from_x: f32,
    to_x: f32,
    timer: Timer,
}

impl FootworkStep {
    /// A [`FOOTWORK_DURATION`] tween between two staged positions.
    fn new(from_x: f32, to_x: f32) -> Self {
        Self {
            from_x,
            to_x,
            timer: Timer::new(FOOTWORK_DURATION, TimerMode::Once),
        }
    }

    fn x(&self, reduced_motion: bool) -> f32 {
        footwork_x(
            self.from_x,
            self.to_x,
            self.timer.fraction(),
            reduced_motion,
        )
    }
}

/// X of a fighter tweening between staged positions at `progress` in
/// `0..=1`: a cubic ease-out from `from_x` landing exactly on `to_x`.
/// Position is semantic state, not decoration, so reduced motion (#200)
/// does not shrink the displacement — it replaces the tween with a
/// near-instant snap to `to_x`. Like [`lunge_x`], a pure function of its
/// inputs, so a flipped `reduced_motion` mid-step is reflected immediately
/// on the very next call.
fn footwork_x(from_x: f32, to_x: f32, progress: f32, reduced_motion: bool) -> f32 {
    if reduced_motion {
        return to_x;
    }
    let progress = progress.clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - progress).powi(3);
    from_x + (to_x - from_x) * eased
}

/// Swaps a fighter onto `clip`: restarts the animation and snaps the atlas
/// index to the clip's first frame.
fn set_clip(
    clip: FighterClip,
    slot: &mut FighterClip,
    anim: &mut SpriteAnimation,
    sprite: Option<&mut Sprite>,
) {
    *slot = clip;
    *anim = clip.animation();
    if let Some(atlas) = sprite.and_then(|sprite| sprite.texture_atlas.as_mut()) {
        atlas.index = anim.first;
    }
}

/// Query alias for the animation-facing components of one fighter side.
type SideAnimation<'w, 's, Side, Other> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut FighterClip,
        &'static mut SpriteAnimation,
        Option<&'static mut Sprite>,
        &'static mut CutoutPose,
    ),
    (With<Side>, Without<Other>),
>;

/// Three-phase pose envelope (§6 of the combat redesign): every non-idle
/// [`CutoutPose`] now plays anticipation → impact → recovery instead of
/// snapping to a single keyframe. The component stores the per-phase
/// durations and one total timer; [`apply_cutout_poses`] turns the elapsed
/// time into eased blend weights over the pose's anticipation and main
/// keyframes (see [`PoseEnvelope::weights`]).
#[derive(Component, Debug, Clone)]
struct PoseEnvelope {
    /// Total envelope clock. For a holding envelope (Knockdown) the pose
    /// stays at the full main keyframe once the timer finishes.
    timer: Timer,
    /// Anticipation phase length, seconds (idle → anticipation key).
    anticipation: f32,
    /// Impact phase length, seconds (anticipation key → main key).
    impact: f32,
    /// Whether the envelope holds the main keyframe forever once finished
    /// (Knockdown) instead of returning the fighter to idle.
    hold: bool,
}

/// Attack anticipation phase: the wind-up back before the strike.
const ATTACK_ANTICIPATION_SECONDS: f32 = 0.120;
/// Attack impact phase: wind-up key to full strike extension. Ends exactly
/// [`ATTACK_ANTICIPATION_SECONDS`] + this = 0.2 s into the attack — the
/// lunge peak, since [`AttackLunge`] is timed to the 0.4 s attack clip and
/// [`lunge_x`] peaks at progress 0.5. Recovery is the remaining clip time.
const ATTACK_IMPACT_SECONDS: f32 = 0.080;
/// Hurt anticipation: near-instant ramp to the overshoot recoil.
const HURT_ANTICIPATION_SECONDS: f32 = 0.060;
/// Hurt impact: overshoot settles back onto the held recoil key.
const HURT_IMPACT_SECONDS: f32 = 0.090;
/// Hurt total: sharp recoil then settle, releasing well inside the 0.5 s
/// presentation gate.
const HURT_TOTAL_SECONDS: f32 = 0.360;
/// Block/Dodge anticipation: the quick brace / lean, then held.
const BRACE_ANTICIPATION_SECONDS: f32 = 0.070;
/// Step anticipation: lean into the [`FOOTWORK_DURATION`] position tween.
const STEP_ANTICIPATION_SECONDS: f32 = 0.100;
/// Step recovery: release the lean just as the tween lands.
const STEP_RECOVERY_SECONDS: f32 = 0.100;
/// Knockdown anticipation: the stagger before the fall.
const KNOCKDOWN_ANTICIPATION_SECONDS: f32 = 0.140;
/// Knockdown impact: stagger key to the ground sprawl, then holds forever.
const KNOCKDOWN_IMPACT_SECONDS: f32 = 0.320;

impl PoseEnvelope {
    fn new(total: f32, anticipation: f32, impact: f32, hold: bool) -> Self {
        Self {
            timer: Timer::from_seconds(total, TimerMode::Once),
            anticipation,
            impact,
            hold,
        }
    }

    /// The envelope for `pose`, or `None` for idle (no envelope: the rig
    /// rests, plus the breathing sway).
    fn for_pose(pose: CutoutPose) -> Option<Self> {
        match pose {
            CutoutPose::Idle => None,
            CutoutPose::Attack => Some(Self::new(
                FighterClip::Attack
                    .animation()
                    .clip_duration()
                    .as_secs_f32(),
                ATTACK_ANTICIPATION_SECONDS,
                ATTACK_IMPACT_SECONDS,
                false,
            )),
            CutoutPose::HitReaction => Some(Self::new(
                HURT_TOTAL_SECONDS,
                HURT_ANTICIPATION_SECONDS,
                HURT_IMPACT_SECONDS,
                false,
            )),
            CutoutPose::Block | CutoutPose::Dodge => Some(Self::new(
                RIG_REACTION_DURATION.as_secs_f32(),
                BRACE_ANTICIPATION_SECONDS,
                0.0,
                false,
            )),
            CutoutPose::StepForward | CutoutPose::StepBack => Some(Self::new(
                FOOTWORK_DURATION.as_secs_f32(),
                STEP_ANTICIPATION_SECONDS,
                FOOTWORK_DURATION.as_secs_f32() - STEP_ANTICIPATION_SECONDS - STEP_RECOVERY_SECONDS,
                false,
            )),
            CutoutPose::Knockdown => Some(Self::new(
                KNOCKDOWN_ANTICIPATION_SECONDS + KNOCKDOWN_IMPACT_SECONDS,
                KNOCKDOWN_ANTICIPATION_SECONDS,
                KNOCKDOWN_IMPACT_SECONDS,
                true,
            )),
        }
    }

    /// `(anticipation_weight, main_weight)` blend over the two keyframes at
    /// the current envelope time:
    ///
    /// - anticipation: idle → anticipation key, cubic ease-out;
    /// - impact: anticipation key → main key, cubic ease-out (sharp);
    /// - recovery: main key → idle, smooth ease-in-out;
    /// - a finished holding envelope stays at the full main key.
    ///
    /// Reduced motion (#200) never sees these weights —
    /// [`apply_cutout_poses`] pins the pose at the full main key for the
    /// envelope's whole (identical) duration instead, preserving the
    /// pre-envelope snap treatment; only presentation blending changes with
    /// the preference, never timing.
    fn weights(&self) -> (f32, f32) {
        let t = self.timer.elapsed_secs();
        if self.timer.is_finished() {
            return if self.hold { (0.0, 1.0) } else { (0.0, 0.0) };
        }
        if t < self.anticipation {
            (ease_out_cubic(t / self.anticipation), 0.0)
        } else if t < self.anticipation + self.impact {
            let s = ease_out_cubic((t - self.anticipation) / self.impact);
            (1.0 - s, s)
        } else {
            let recovery = (self.timer.duration().as_secs_f32() - self.anticipation - self.impact)
                .max(f32::EPSILON);
            let r = (t - self.anticipation - self.impact) / recovery;
            if self.hold {
                (0.0, 1.0)
            } else {
                (0.0, 1.0 - ease_in_out_cubic(r.clamp(0.0, 1.0)))
            }
        }
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn set_cutout_pose(
    commands: &mut Commands,
    entity: Entity,
    pose: CutoutPose,
    slot: &mut CutoutPose,
) {
    *slot = pose;
    match PoseEnvelope::for_pose(pose) {
        Some(envelope) => {
            commands.entity(entity).insert(envelope);
        }
        None => {
            commands.entity(entity).remove::<PoseEnvelope>();
        }
    }
}

/// Maps this frame's combat events onto clips: any strike attempt plays the
/// attacker's attack (with a lunge), miss/reach failures make the defender
/// avoid, Hit/Crit/Blocked plays the defender's reaction, and Defeated plays
/// the defender's KO (which then freezes). Movement events additionally
/// advance [`ArenaStaging`] and tween the actor (and, on a pair slide, the
/// opponent) to the new staged x.
fn animate_combat_events(
    mut commands: Commands,
    mut events: MessageReader<CombatLogEvent>,
    mut staging: ResMut<ArenaStaging>,
    mut players: SideAnimation<PlayerFighter, EnemyFighter>,
    mut enemies: SideAnimation<EnemyFighter, PlayerFighter>,
) {
    for CombatLogEvent {
        actor,
        action,
        event,
    } in events.read().copied()
    {
        let (attacker, defender) = match actor {
            CombatSide::Player => (players.single_mut(), enemies.single_mut()),
            CombatSide::Enemy => (enemies.single_mut(), players.single_mut()),
        };
        match event {
            CombatEvent::Missed
            | CombatEvent::OutOfReach
            | CombatEvent::Hit { .. }
            | CombatEvent::Crit { .. }
            | CombatEvent::Blocked { .. } => {
                if let Ok((entity, mut clip, mut anim, mut sprite, mut pose)) = attacker {
                    set_clip(
                        FighterClip::Attack,
                        &mut clip,
                        &mut anim,
                        sprite.as_deref_mut(),
                    );
                    set_cutout_pose(&mut commands, entity, CutoutPose::Attack, &mut pose);
                    commands.entity(entity).insert(AttackLunge::for_side(actor));
                }
                if matches!(event, CombatEvent::Missed | CombatEvent::OutOfReach)
                    && let Ok((entity, _, _, _, mut pose)) = defender
                {
                    set_cutout_pose(&mut commands, entity, CutoutPose::Dodge, &mut pose);
                } else if !matches!(event, CombatEvent::Missed | CombatEvent::OutOfReach)
                    && let Ok((entity, mut clip, mut anim, mut sprite, mut pose)) = defender
                    && *clip != FighterClip::Ko
                {
                    set_clip(
                        FighterClip::Hurt,
                        &mut clip,
                        &mut anim,
                        sprite.as_deref_mut(),
                    );
                    let pose_kind = match event {
                        CombatEvent::Blocked { .. } => CutoutPose::Block,
                        _ => CutoutPose::HitReaction,
                    };
                    set_cutout_pose(&mut commands, entity, pose_kind, &mut pose);
                }
            }
            CombatEvent::Defeated => {
                if let Ok((entity, mut clip, mut anim, mut sprite, mut pose)) = defender {
                    set_clip(FighterClip::Ko, &mut clip, &mut anim, sprite.as_deref_mut());
                    set_cutout_pose(&mut commands, entity, CutoutPose::Knockdown, &mut pose);
                }
            }
            CombatEvent::Guarded => {
                if let Ok((entity, _, _, _, mut pose)) = attacker {
                    set_cutout_pose(&mut commands, entity, CutoutPose::Block, &mut pose);
                }
            }
            CombatEvent::Rested { .. } | CombatEvent::OutOfStamina => {}
            CombatEvent::Moved { to, .. } => {
                // The staging update is combat truth and applies regardless
                // of presentation state; the tweens below only pace it.
                let old_actor_x = staging.x_of(actor);
                let old_opponent_x = staging.x_of(actor.opponent());
                staging.apply_move(actor, to);
                let clip = match action {
                    CombatAction::StepBack => FighterClip::StepBack,
                    CombatAction::StepForward | CombatAction::LeapForward => {
                        FighterClip::StepForward
                    }
                    _ => FighterClip::StepForward,
                };
                if let Ok((entity, mut current, mut anim, mut sprite, mut pose)) = attacker
                    && *current != FighterClip::Ko
                {
                    set_clip(clip, &mut current, &mut anim, sprite.as_deref_mut());
                    let pose_kind = match clip {
                        FighterClip::StepBack => CutoutPose::StepBack,
                        _ => CutoutPose::StepForward,
                    };
                    set_cutout_pose(&mut commands, entity, pose_kind, &mut pose);
                    commands
                        .entity(entity)
                        .insert(FootworkStep::new(old_actor_x, staging.x_of(actor)));
                }
                // A wall hit slides both fighters (see
                // `ArenaStaging::apply_move`); the standing opponent glides
                // to its new x with the same tween, no clip change.
                let new_opponent_x = staging.x_of(actor.opponent());
                if new_opponent_x != old_opponent_x
                    && let Ok((entity, _, _, _, _)) = defender
                {
                    commands
                        .entity(entity)
                        .insert(FootworkStep::new(old_opponent_x, new_opponent_x));
                }
            }
        }
    }
}

/// Advances every pose envelope and returns finished non-holding poses to
/// idle. The sprite-sheet clip system remains authoritative for root clip
/// state; this only clears jointed body poses. A holding envelope
/// (Knockdown) keeps both its pose and its finished envelope so the fighter
/// stays on the ground.
/// A hit-stopped fighter (`fx::HitStop`, ~70 ms) freezes its pose envelope,
/// sprite frames, and footwork mid-flight; the `Without` filters below
/// simply skip ticking it. The opponent and the turn engine never freeze.
fn tick_pose_envelopes(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut CutoutPose, &mut PoseEnvelope), Without<super::fx::HitStop>>,
) {
    for (entity, mut pose, mut envelope) in &mut query {
        envelope.timer.tick(time.delta());
        if envelope.timer.is_finished() && !envelope.hold {
            *pose = CutoutPose::Idle;
            commands.entity(entity).remove::<PoseEnvelope>();
        }
    }
}

/// Ticks every [`SpriteAnimation`] and writes the advanced frame into the
/// sprite's atlas index.
fn advance_animations(
    time: Res<Time>,
    mut query: Query<(&mut SpriteAnimation, Option<&mut Sprite>), Without<super::fx::HitStop>>,
) {
    for (mut anim, sprite) in &mut query {
        if let Some(mut sprite) = sprite
            && let Some(atlas) = sprite.texture_atlas.as_mut()
        {
            if let Some(frame) = anim.advance(time.delta(), atlas.index) {
                atlas.index = frame;
            }
        } else {
            let current = anim.current_frame();
            anim.advance(time.delta(), current);
        }
    }
}

/// Returns every finished `Once` clip to the idle loop — except KO, which
/// stays frozen on its last frame.
fn return_to_idle(mut query: Query<(&mut FighterClip, &mut SpriteAnimation, Option<&mut Sprite>)>) {
    for (mut clip, mut anim, mut sprite) in &mut query {
        if anim.is_finished() && *clip != FighterClip::Ko {
            set_clip(
                FighterClip::Idle,
                &mut clip,
                &mut anim,
                sprite.as_deref_mut(),
            );
        }
    }
}

/// Query data alias for the fighter-root state [`apply_cutout_poses`] reads:
/// the pose, its envelope (if any), the rig (for mirroring), and whether the
/// fighter is currently hit-stopped.
type PosedFighter = (
    &'static CutoutPose,
    Option<&'static PoseEnvelope>,
    Option<&'static CutoutRig>,
    Has<super::fx::HitStop>,
);

/// Applies the current jointed pose to every body-part child, rebuilding from
/// the part's neutral transform so gear parented beneath hands/arms/shields
/// inherits the same motion without independent drift.
///
/// Forearms, hands, shins, and feet are nested several joints deep under
/// their own parent part rather than being direct children of the fighter
/// root (#117), so the owning fighter is found by climbing the chain via
/// [`cutout_rig_owner`] instead of assuming a single `ChildOf` hop.
fn apply_cutout_poses(
    time: Res<Time>,
    accessibility: Res<AccessibilityPreferences>,
    fighters: Query<PosedFighter>,
    ancestry: Query<&ChildOf, With<CutoutPartMarker>>,
    mut parts: Query<(
        &CutoutPartMarker,
        &ChildOf,
        &CutoutPartRestPose,
        &mut Transform,
    )>,
) {
    let breath_phase = if accessibility.reduced_motion {
        None
    } else {
        Some(time.elapsed_secs())
    };
    for (marker, child_of, rest, mut transform) in &mut parts {
        let root = cutout_rig_owner(child_of.parent(), |entity| {
            ancestry.get(entity).ok().map(|child_of| child_of.parent())
        });
        let Ok((pose, envelope, rig, hit_stopped)) = fighters.get(root) else {
            continue;
        };
        // A hit-stopped fighter is fully frozen: no breathing sway either.
        let breath_phase = if hit_stopped { None } else { breath_phase };
        // Reduced motion (#200) pins the pose at the full main keyframe for
        // the envelope's whole duration — the pre-envelope treatment: state
        // is readable immediately, no blended in-between frames. Timing is
        // identical either way (the envelope timer ticks the same).
        let weights = match (*pose, envelope, accessibility.reduced_motion) {
            // Idle carries no keyframe: full weight would only zero out the
            // breathing sway's fade factor below.
            (CutoutPose::Idle, _, _) => (0.0, 0.0),
            // Reduced motion, or a pose set without an envelope: pinned at
            // the full main key, the pre-envelope treatment.
            (_, Some(_), true) | (_, None, _) => (0.0, 1.0),
            (_, Some(envelope), false) => envelope.weights(),
        };
        let flip_x = rig.map(|rig| rig.flip_x).unwrap_or(false);
        *transform = posed_part_transform(marker.kind, rest, *pose, weights, breath_phase, flip_x);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct JointedPartDelta {
    offset: Vec2,
    rotation: f32,
}

impl JointedPartDelta {
    fn scaled(self, factor: f32) -> Self {
        Self {
            offset: self.offset * factor,
            rotation: self.rotation * factor,
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            offset: self.offset + other.offset,
            rotation: self.rotation + other.rotation,
        }
    }
}

fn posed_part_transform(
    kind: crate::cutout::CutoutPartKind,
    rest: &CutoutPartRestPose,
    pose: CutoutPose,
    (anticipation_weight, main_weight): (f32, f32),
    breath_phase: Option<f32>,
    flip_x: bool,
) -> Transform {
    let mut delta = anticipation_part_delta(kind, pose)
        .scaled(anticipation_weight)
        .plus(jointed_part_delta(kind, pose).scaled(main_weight));
    // The breathing sway rides on top of (and fades out against) any active
    // pose so idle never pops when a pose starts or releases.
    if let Some(phase) = breath_phase {
        let pose_presence = (anticipation_weight + main_weight).clamp(0.0, 1.0);
        delta = delta.plus(idle_breath_delta(kind, phase).scaled(1.0 - pose_presence));
    }
    if flip_x {
        delta.offset.x = -delta.offset.x;
        delta.rotation = -delta.rotation;
    }

    let mut transform = rest.transform;
    let pivot_shift = pivot_shift(kind, rest.size, delta.rotation);
    let rest_angle = transform.rotation.to_euler(EulerRot::XYZ).2;
    let pivot_shift = rotate_vec2(pivot_shift, rest_angle);
    transform.translation.x += delta.offset.x + pivot_shift.x;
    transform.translation.y += delta.offset.y + pivot_shift.y;
    transform.rotation *= Quat::from_rotation_z(delta.rotation);
    transform
}

fn pivot_shift(kind: crate::cutout::CutoutPartKind, size: Vec2, angle: f32) -> Vec2 {
    let pivot = joint_pivot(kind, size);
    pivot - rotate_vec2(pivot, angle)
}

fn joint_pivot(kind: crate::cutout::CutoutPartKind, size: Vec2) -> Vec2 {
    use crate::cutout::CutoutPartKind::*;
    match kind {
        UpperArmBack | UpperArmFront | ForearmBack | ForearmFront | ThighBack | ThighFront
        | ShinBack | ShinFront => Vec2::new(0.0, size.y * 0.42),
        HandBack | HandFront => Vec2::new(0.0, size.y * 0.28),
        FootBack | FootFront => Vec2::new(-size.x * 0.28, 0.0),
        Torso => Vec2::new(0.0, -size.y * 0.34),
        Head | Hair => Vec2::new(0.0, -size.y * 0.38),
    }
}

fn rotate_vec2(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

fn jointed_part_delta(kind: crate::cutout::CutoutPartKind, pose: CutoutPose) -> JointedPartDelta {
    use crate::cutout::CutoutPartKind::*;
    let (x, y, rotation) = match pose {
        CutoutPose::Idle => (0.0, 0.0, 0.0),
        CutoutPose::Attack => match kind {
            UpperArmFront => (8.0, 3.0, -0.64),
            ForearmFront => (17.0, 7.0, -0.92),
            HandFront => (24.0, 9.0, -0.48),
            UpperArmBack => (-3.0, 3.0, 0.34),
            ForearmBack => (-5.0, 6.0, 0.48),
            HandBack => (-4.0, 7.0, 0.2),
            Torso => (3.0, 0.0, -0.08),
            Head | Hair => (4.0, 0.0, -0.06),
            ThighFront | ShinBack => (2.0, -1.0, -0.12),
            ThighBack | ShinFront => (-1.0, 1.0, 0.1),
            FootFront => (3.0, -1.0, 0.04),
            FootBack => (-1.0, 0.0, -0.04),
        },
        CutoutPose::Block => match kind {
            UpperArmFront => (-8.0, 11.0, 0.58),
            ForearmFront => (-14.0, 21.0, 1.05),
            HandFront => (-16.0, 28.0, 0.78),
            UpperArmBack => (2.0, 8.0, -0.32),
            ForearmBack => (7.0, 15.0, -0.5),
            HandBack => (8.0, 20.0, -0.3),
            Torso => (-2.0, -1.0, 0.08),
            Head | Hair => (-3.0, 1.0, 0.05),
            _ => (0.0, 0.0, 0.0),
        },
        CutoutPose::Dodge => match kind {
            Torso => (-12.0, -2.0, 0.24),
            Head | Hair => (-18.0, -1.0, 0.28),
            UpperArmFront | ForearmFront | HandFront => (-13.0, -2.0, 0.34),
            UpperArmBack | ForearmBack | HandBack => (-10.0, 2.0, 0.2),
            ThighFront | ShinFront | FootFront => (5.0, -1.0, -0.18),
            ThighBack | ShinBack | FootBack => (-7.0, 1.0, 0.16),
        },
        CutoutPose::HitReaction => match kind {
            Torso => (-8.0, -2.0, 0.18),
            Head | Hair => (-13.0, -4.0, 0.26),
            UpperArmFront | ForearmFront | HandFront => (-11.0, 4.0, 0.5),
            UpperArmBack | ForearmBack | HandBack => (-8.0, 2.0, -0.34),
            ThighFront | ShinFront | FootFront => (-3.0, -1.0, -0.06),
            ThighBack | ShinBack | FootBack => (2.0, 0.0, 0.08),
        },
        // Ground sprawl re-derived chain-length-relatively for the phase-4
        // folk proportions (the old deltas predated the #117 joint nesting
        // and the leg-length pass, leaving limbs detached and the sprawl
        // floating). Root-level parts (torso, head, hair, upper arms,
        // thighs) are placed so the body lies along the rest-pose ground
        // line (foot bottom ≈ 104.8 rig units below the root); chained
        // parts (forearms, hands, shins, feet) get rotation-only deltas —
        // their rest offsets already sit on the parent joint pivot, so
        // transform propagation keeps them glued while `pivot_shift` turns
        // the rotation into a joint bend.
        CutoutPose::Knockdown => match kind {
            Torso => (2.0, -67.4, 1.35),
            Head => (-62.7, -120.8, 1.38),
            Hair => (-83.8, -143.7, 1.40),
            UpperArmBack => (-46.0, -134.1, -1.39),
            ForearmBack => (0.0, 0.0, -0.2),
            HandBack => (0.0, 0.0, -0.1),
            UpperArmFront => (-78.4, -92.8, 1.37),
            ForearmFront => (0.0, 0.0, 0.25),
            HandFront => (0.0, 0.0, 0.15),
            ThighFront => (-5.1, -47.0, 1.17),
            ShinFront => (0.0, 0.0, -0.22),
            FootFront => (0.0, 0.0, -0.8),
            ThighBack => (20.5, -74.0, 1.2),
            ShinBack => (0.0, 0.0, 0.35),
            FootBack => (0.0, 0.0, -0.1),
        },
        CutoutPose::StepForward => match kind {
            UpperArmFront | ForearmFront | HandFront => (-4.0, 0.0, 0.22),
            UpperArmBack | ForearmBack | HandBack => (5.0, 0.0, -0.22),
            ThighFront => (7.0, 0.0, -0.26),
            ShinFront => (12.0, -1.0, -0.2),
            FootFront => (15.0, -1.0, 0.08),
            ThighBack => (-5.0, 1.0, 0.2),
            ShinBack => (-8.0, 1.0, 0.18),
            FootBack => (-10.0, 0.0, -0.06),
            Torso | Head | Hair => (2.0, 0.0, -0.03),
        },
        CutoutPose::StepBack => match kind {
            UpperArmFront | ForearmFront | HandFront => (5.0, 0.0, -0.22),
            UpperArmBack | ForearmBack | HandBack => (-4.0, 0.0, 0.22),
            ThighFront => (-6.0, 1.0, 0.22),
            ShinFront => (-10.0, 0.0, 0.18),
            FootFront => (-12.0, 0.0, -0.08),
            ThighBack => (7.0, 0.0, -0.24),
            ShinBack => (11.0, -1.0, -0.2),
            FootBack => (14.0, -1.0, 0.08),
            Torso | Head | Hair => (-2.0, 0.0, 0.04),
        },
    };
    JointedPartDelta {
        offset: Vec2::new(x, y),
        rotation,
    }
}

/// The anticipation keyframe of `pose` — what the envelope blends towards
/// during its anticipation phase, before crossing to the main
/// [`jointed_part_delta`] key at impact.
///
/// - Attack winds up: arms pulled back, slight counter-lean.
/// - HitReaction overshoots the held recoil, so the settle reads as a sharp
///   snap-then-ease.
/// - Block/Dodge/Steps brace straight into the main key (the envelope's
///   anticipation phase is the quick ramp, impact holds it).
/// - Knockdown staggers upright before the fall.
fn anticipation_part_delta(
    kind: crate::cutout::CutoutPartKind,
    pose: CutoutPose,
) -> JointedPartDelta {
    use crate::cutout::CutoutPartKind::*;
    /// How far past the held recoil the hurt overshoot key reaches.
    const HURT_OVERSHOOT: f32 = 1.22;
    let (x, y, rotation) = match pose {
        CutoutPose::Idle => (0.0, 0.0, 0.0),
        CutoutPose::Attack => match kind {
            UpperArmFront => (-5.0, 2.0, 0.42),
            ForearmFront => (-9.0, 4.0, 0.55),
            HandFront => (-11.0, 5.0, 0.3),
            UpperArmBack => (2.0, 1.0, -0.2),
            ForearmBack => (3.0, 2.0, -0.26),
            HandBack => (3.0, 3.0, -0.12),
            Torso => (-3.0, -1.0, 0.07),
            Head | Hair => (-4.0, 0.0, 0.05),
            ThighFront | ShinBack => (-1.0, -1.0, 0.06),
            ThighBack | ShinFront => (1.0, 0.0, -0.05),
            FootFront => (-1.0, 0.0, -0.02),
            FootBack => (1.0, 0.0, 0.02),
        },
        CutoutPose::HitReaction => {
            return jointed_part_delta(kind, pose).scaled(HURT_OVERSHOOT);
        }
        CutoutPose::Block | CutoutPose::Dodge | CutoutPose::StepForward | CutoutPose::StepBack => {
            return jointed_part_delta(kind, pose);
        }
        // The stagger: a hard backward lean with flailing arms, still
        // upright. Chained parts bend rotation-only (see the Knockdown main
        // key in [`jointed_part_delta`]) so the limbs stay glued mid-fall.
        CutoutPose::Knockdown => match kind {
            Torso => (-10.0, -3.0, 0.3),
            Head => (-16.0, -3.0, 0.34),
            Hair => (-17.0, -2.0, 0.34),
            UpperArmFront => (-8.0, 4.0, 0.5),
            ForearmFront => (0.0, 0.0, 0.3),
            HandFront => (0.0, 0.0, 0.2),
            UpperArmBack => (-5.0, 3.0, -0.3),
            ForearmBack => (0.0, 0.0, -0.25),
            HandBack => (0.0, 0.0, -0.15),
            ThighFront => (4.0, -2.0, -0.15),
            ShinFront => (0.0, 0.0, -0.08),
            FootFront => (0.0, 0.0, 0.05),
            ThighBack => (-4.0, -2.0, 0.12),
            ShinBack => (0.0, 0.0, 0.08),
            FootBack => (0.0, 0.0, -0.04),
        },
    };
    JointedPartDelta {
        offset: Vec2::new(x, y),
        rotation,
    }
}

/// Angular frequency of the idle breathing sway, rad/s (~4 s per breath).
const BREATH_FREQUENCY: f32 = 1.55;
/// Peak torso rise of the breathing sway, in rig units — deliberately tiny.
const BREATH_TORSO_AMPLITUDE: f32 = 0.8;

/// The subtle idle breathing sway: torso and head rise and settle a hair,
/// arms follow with a slight lag. Computed fresh from the elapsed-time
/// `phase` every frame (never accumulated), so frozen virtual time (the
/// baseline freeze fixtures) holds it perfectly still — the same pattern as
/// `fx::drift_parallax_layers`. Reduced motion (#200) disables it entirely
/// (the caller passes no phase); legs stay planted in either mode.
fn idle_breath_delta(kind: crate::cutout::CutoutPartKind, phase: f32) -> JointedPartDelta {
    use crate::cutout::CutoutPartKind::*;
    let breath = (phase * BREATH_FREQUENCY).sin();
    let lagged = (phase * BREATH_FREQUENCY - 0.6).sin();
    let (x, y, rotation) = match kind {
        Torso => (0.0, BREATH_TORSO_AMPLITUDE * breath, 0.0),
        Head => (0.0, 1.1 * lagged, 0.006 * breath),
        Hair => (0.0, 1.2 * lagged, 0.006 * breath),
        UpperArmFront | ForearmFront | HandFront => (0.0, 0.5 * lagged, 0.008 * breath),
        UpperArmBack | ForearmBack | HandBack => (0.0, 0.5 * lagged, -0.008 * breath),
        ThighFront | ThighBack | ShinFront | ShinBack | FootFront | FootBack => (0.0, 0.0, 0.0),
    };
    JointedPartDelta {
        offset: Vec2::new(x, y),
        rotation,
    }
}

/// Tweens lunging fighters along [`lunge_x`] and lands them back exactly on
/// their *current* staged x when the lunge ends — endpoints are read live
/// from [`ArenaStaging`], never a fixed anchor. The lunge's own timer
/// (paced by the attack clip's duration, see [`AttackLunge::for_side`])
/// ticks identically regardless of
/// [`AccessibilityPreferences::reduced_motion`] -- only the peak
/// displacement `lunge_x` computes changes; presentation timing never does.
fn apply_lunges(
    time: Res<Time>,
    mut commands: Commands,
    accessibility: Res<AccessibilityPreferences>,
    staging: Res<ArenaStaging>,
    mut query: Query<(Entity, &mut AttackLunge, &mut Transform)>,
) {
    for (entity, mut lunge, mut transform) in &mut query {
        lunge.timer.tick(time.delta());
        let from_x = staging.x_of(lunge.side);
        if lunge.timer.is_finished() {
            transform.translation.x = from_x;
            commands.entity(entity).remove::<AttackLunge>();
        } else {
            transform.translation.x = lunge_x(
                from_x,
                staging.x_of(lunge.side.opponent()),
                lunge.timer.fraction(),
                accessibility.reduced_motion,
            );
        }
    }
}

/// Applies the movement tweens and lands fighters exactly on their new
/// staged x at the end. Same timing invariant as [`apply_lunges`]:
/// [`FOOTWORK_DURATION`] never changes with the preference — under reduced
/// motion the fighter simply sits on `to_x` from the first frame (see
/// [`footwork_x`]), because the staged position is semantic state.
fn apply_footwork(
    time: Res<Time>,
    mut commands: Commands,
    accessibility: Res<AccessibilityPreferences>,
    mut query: Query<(Entity, &mut FootworkStep, &mut Transform), Without<super::fx::HitStop>>,
) {
    for (entity, mut footwork, mut transform) in &mut query {
        footwork.timer.tick(time.delta());
        if footwork.timer.is_finished() {
            transform.translation.x = footwork.to_x;
            commands.entity(entity).remove::<FootworkStep>();
        } else {
            transform.translation.x = footwork.x(accessibility.reduced_motion);
        }
    }
}

/// Registers the sheet loading and the animation systems; added by the
/// arena plugin.
pub(super) struct AnimationPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AnimationSet {
    Apply,
}

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        // Idempotent with CombatPlugin's registration; keeps the arena
        // usable in apps built without the combat plugin (tests).
        app.add_message::<CombatLogEvent>();
        // Idempotent with SettingsPlugin's registration (#200): keeps the
        // reduced-motion systems below usable in apps/tests built without
        // it, defaulting to full motion.
        app.init_resource::<AccessibilityPreferences>();
        // Idempotent with ArenaPlugin's registration: the staging-driven
        // systems below never observe a missing resource.
        app.init_resource::<ArenaStaging>();
        app.add_systems(Startup, load_fighter_sheets).add_systems(
            Update,
            (
                animate_combat_events,
                advance_animations,
                return_to_idle,
                tick_pose_envelopes,
                apply_cutout_poses,
                apply_lunges,
                apply_footwork,
            )
                .chain()
                .in_set(AnimationSet::Apply)
                .run_if(in_state(crate::core::GameState::Fight)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::arena::staging::{CLOSE_GAP, FAR_GAP, NEAR_GAP};
    use crate::character::{Attributes, Fighter};
    use crate::combat::DuelDistance;
    use crate::core::{CorePlugin, GameState};
    use crate::creation::PlayerCharacter;
    use crate::cutout::{CutoutPartKind, CutoutPartMarker, CutoutPose};
    use bevy::state::app::StatesPlugin;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn a_loop_clip_advances_one_frame_per_period_and_wraps() {
        let mut anim = SpriteAnimation::new(0, 3, 8.0, AnimationMode::Loop);
        // 8 fps -> 125 ms per frame; just short of the period does nothing.
        assert_eq!(anim.advance(ms(124), 0), None);
        assert_eq!(anim.advance(ms(1), 0), Some(1));
        assert_eq!(anim.advance(ms(125), 1), Some(2));
        assert_eq!(anim.advance(ms(125), 2), Some(3));
        assert_eq!(anim.advance(ms(125), 3), Some(0), "loops back to first");
        assert!(!anim.is_finished(), "loops never finish");
    }

    #[test]
    fn a_large_delta_advances_multiple_frames() {
        let mut anim = SpriteAnimation::new(0, 3, 8.0, AnimationMode::Loop);
        assert_eq!(anim.advance(ms(375), 0), Some(3), "three periods at once");
    }

    #[test]
    fn a_once_clip_shows_its_last_frame_then_finishes_and_freezes() {
        let mut anim = SpriteAnimation::new(4, 7, 10.0, AnimationMode::Once);
        let mut frame = 4;
        for expected in [5, 6, 7] {
            frame = anim.advance(ms(101), frame).expect("advances");
            assert_eq!(frame, expected);
            assert!(!anim.is_finished());
        }
        // The last frame plays out its full duration before finishing.
        assert_eq!(anim.advance(ms(101), frame), None);
        assert!(anim.is_finished());
        assert_eq!(anim.advance(ms(1000), frame), None, "frozen when finished");
    }

    #[test]
    fn the_clip_table_tiles_the_sheet_without_gaps() {
        let idle = FighterClip::Idle.animation();
        let attack = FighterClip::Attack.animation();
        let hurt = FighterClip::Hurt.animation();
        let ko = FighterClip::Ko.animation();
        let step_forward = FighterClip::StepForward.animation();
        let step_back = FighterClip::StepBack.animation();
        assert_eq!((idle.first, idle.last), (0, 3));
        assert_eq!((attack.first, attack.last), (4, 7));
        assert_eq!((hurt.first, hurt.last), (8, 9));
        assert_eq!((ko.first, ko.last), (10, 11));
        assert_eq!((step_forward.first, step_forward.last), (12, 13));
        assert_eq!((step_back.first, step_back.last), (14, 15));
        assert_eq!(step_back.last as u32 + 1, ATLAS_COLUMNS * ATLAS_ROWS);
        assert_eq!(idle.mode, AnimationMode::Loop);
        for once in [attack, hurt, ko, step_forward, step_back] {
            assert_eq!(once.mode, AnimationMode::Once);
        }
    }

    #[test]
    fn the_lunge_arcs_out_and_back_from_the_staged_position() {
        let staging = ArenaStaging::starting();
        let from = staging.player_x;
        let toward = staging.enemy_x;
        assert_eq!(lunge_x(from, toward, 0.0, false), from);
        assert_eq!(lunge_x(from, toward, 1.0, false), from);
        let peak = lunge_x(from, toward, 0.5, false);
        assert!(peak > from, "the player lunges rightwards");
        assert!(
            (peak - (from + staging.gap() * LUNGE_FRACTION)).abs() < 1e-3,
            "peaks at the lunge fraction of the current staged gap"
        );
        let quarter = lunge_x(from, toward, 0.25, false);
        assert!(from < quarter && quarter < peak, "smooth arc out");
    }

    #[test]
    fn the_lunge_peak_scales_with_the_current_gap() {
        let mut staging = ArenaStaging::starting();
        let close_peak = lunge_x(staging.player_x, staging.enemy_x, 0.5, false);
        assert!((close_peak - staging.player_x - CLOSE_GAP * LUNGE_FRACTION).abs() < 1e-3);
        staging.apply_move(CombatSide::Player, DuelDistance::FAR);
        let far_peak = lunge_x(staging.player_x, staging.enemy_x, 0.5, false);
        assert!(
            (far_peak - staging.player_x - FAR_GAP * LUNGE_FRACTION).abs() < 1e-3,
            "a wider band lunges proportionally further"
        );
    }

    #[test]
    fn reduced_motion_shrinks_the_lunge_to_the_documented_nudge_on_the_same_arc() {
        let staging = ArenaStaging::starting();
        let from = staging.player_x;
        let toward = staging.enemy_x;
        assert_eq!(
            lunge_x(from, toward, 0.0, true),
            from,
            "endpoints stay exactly on the staged x in either mode"
        );
        assert_eq!(lunge_x(from, toward, 1.0, true), from);
        let peak = lunge_x(from, toward, 0.5, true);
        assert!(peak > from, "still lunges towards the opponent");
        assert!(
            (peak - from - REDUCED_MOTION_DISPLACEMENT).abs() < 1e-3,
            "peaks at exactly the documented reduced-motion nudge, not the lunge fraction"
        );
        let full_peak = lunge_x(from, toward, 0.5, false);
        assert!(
            peak < full_peak,
            "reduced motion is a strictly smaller displacement than full motion"
        );
    }

    /// Headless app inside the fight with both fighters spawned as animated
    /// roots.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes::default(),
            appearance: crate::character::PlayerAppearance::default(),
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app
    }

    fn write_event(app: &mut App, actor: CombatSide, event: CombatEvent) {
        write_event_with_action(app, actor, crate::combat::CombatAction::QuickStrike, event);
    }

    fn write_event_with_action(
        app: &mut App,
        actor: CombatSide,
        action: crate::combat::CombatAction,
        event: CombatEvent,
    ) {
        app.world_mut().write_message(CombatLogEvent {
            actor,
            action,
            event,
        });
        app.update();
    }

    fn side_state<M: Component>(app: &mut App) -> (FighterClip, usize) {
        let (clip, anim) = app
            .world_mut()
            .query_filtered::<(&FighterClip, &SpriteAnimation), With<M>>()
            .single(app.world())
            .expect("fighter exists");
        (*clip, anim.current_frame())
    }

    fn rig_pose<M: Component>(app: &mut App) -> CutoutPose {
        *app.world_mut()
            .query_filtered::<&CutoutPose, With<M>>()
            .single(app.world())
            .expect("fighter has a cutout pose")
    }

    /// Finds the [`Transform`] of the body part of `kind` owned by the
    /// fighter tagged `M`. Forearms/hands/shins/feet are nested several
    /// joints deep under their own parent part rather than being direct
    /// children of the fighter root (#117), so ownership is resolved by
    /// climbing the chain via [`cutout_rig_owner`] instead of assuming a
    /// single `ChildOf` hop from the part to the fighter.
    fn part_transform<M: Component>(app: &mut App, kind: CutoutPartKind) -> Transform {
        let world = app.world_mut();
        let parent_of: std::collections::HashMap<Entity, Entity> = world
            .query_filtered::<(Entity, &ChildOf), With<CutoutPartMarker>>()
            .iter(world)
            .map(|(entity, child_of)| (entity, child_of.parent()))
            .collect();
        let mut query = world.query::<(Entity, &CutoutPartMarker, &Transform)>();
        query
            .iter(world)
            .find_map(|(entity, marker, transform)| {
                if marker.kind != kind {
                    return None;
                }
                let root = cutout_rig_owner(entity, |e| parent_of.get(&e).copied());
                world.get::<M>(root).map(|_| *transform)
            })
            .expect("part exists")
    }

    #[test]
    fn fighters_spawn_on_the_idle_clip() {
        let mut app = test_app();
        assert_eq!(side_state::<PlayerFighter>(&mut app).0, FighterClip::Idle);
        assert_eq!(side_state::<EnemyFighter>(&mut app), (FighterClip::Idle, 0));
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Idle);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Idle);
    }

    #[test]
    fn a_strike_plays_the_attackers_attack_and_starts_a_lunge() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        let (clip, index) = side_state::<PlayerFighter>(&mut app);
        assert_eq!(clip, FighterClip::Attack);
        assert_eq!(index, FighterClip::Attack.animation().first);
        assert_eq!(
            side_state::<EnemyFighter>(&mut app).0,
            FighterClip::Idle,
            "a miss leaves the defender alone"
        );
        let lunges = app
            .world_mut()
            .query_filtered::<(), (With<AttackLunge>, With<PlayerFighter>)>()
            .iter(app.world())
            .count();
        assert_eq!(lunges, 1, "the attacker lunges");
    }

    /// Advances the clock by exactly `seconds` and runs a frame -- the same
    /// deterministic-tick helper `arena::fx`'s tests use, needed here so the
    /// reduced-motion displacement assertions below aren't at the mercy of
    /// real wall-clock jitter between `app.update()` calls.
    fn advance(app: &mut App, seconds: f32) {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(seconds),
        ));
        app.update();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
    }

    fn player_transform_x(app: &mut App) -> f32 {
        app.world_mut()
            .query_filtered::<&Transform, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists")
            .translation
            .x
    }

    #[test]
    fn reduced_motion_shrinks_the_attack_lunge_on_the_actual_fighter() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        let staged_x = app.world().resource::<ArenaStaging>().player_x;
        let half_clip = FighterClip::Attack
            .animation()
            .clip_duration()
            .as_secs_f32()
            / 2.0;
        advance(&mut app, half_clip);
        let offset = (player_transform_x(&mut app) - staged_x).abs();
        assert!(
            offset <= REDUCED_MOTION_DISPLACEMENT + 0.5,
            "the lunge stays within the documented reduced-motion nudge: {offset}"
        );
    }

    #[test]
    fn toggling_reduced_motion_on_mid_lunge_shrinks_the_fighter_offset_immediately() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        let staged_x = app.world().resource::<ArenaStaging>().player_x;
        let half_clip = FighterClip::Attack
            .animation()
            .clip_duration()
            .as_secs_f32()
            / 2.0;
        advance(&mut app, half_clip);
        let full_offset = (player_transform_x(&mut app) - staged_x).abs();
        assert!(
            full_offset > REDUCED_MOTION_DISPLACEMENT,
            "full motion lunges past the reduced-motion nudge: {full_offset}"
        );

        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        advance(&mut app, 0.001);
        let reduced_offset = (player_transform_x(&mut app) - staged_x).abs();
        assert!(
            reduced_offset <= REDUCED_MOTION_DISPLACEMENT + 0.5,
            "flipping the preference mid-lunge snaps the fighter's offset down \
             immediately, nothing stuck at the full-motion displacement: {reduced_offset}"
        );
    }

    #[test]
    fn combat_events_map_to_jointed_rig_pose_states() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Hit { dmg: 4 });
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::HitReaction);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 4 });
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::HitReaction);

        let mut app = test_app();
        write_event(
            &mut app,
            CombatSide::Player,
            CombatEvent::Blocked { dmg: 2 },
        );
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Block);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Blocked { dmg: 2 });
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Block);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Guarded);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Block);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Idle);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Guarded);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Block);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Idle);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Missed);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Dodge);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Dodge);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::OutOfReach);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Dodge);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::OutOfReach);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Attack);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Dodge);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Defeated);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Knockdown);

        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Defeated);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Knockdown);

        let mut app = test_app();
        write_event_with_action(
            &mut app,
            CombatSide::Enemy,
            crate::combat::CombatAction::StepBack,
            CombatEvent::Moved {
                from: crate::combat::DuelDistance::CLOSE,
                to: crate::combat::DuelDistance::NEAR,
            },
        );
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::StepBack);
    }

    #[test]
    fn jointed_attack_pose_rotates_cutout_arm_parts_from_neutral() {
        let mut app = test_app();
        let neutral_hand = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::HandFront);
        let neutral_forearm =
            part_transform::<PlayerFighter>(&mut app, CutoutPartKind::ForearmFront);

        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        // Mid-envelope: past the wind-up, inside the strike extension.
        advance(
            &mut app,
            ATTACK_ANTICIPATION_SECONDS + ATTACK_IMPACT_SECONDS,
        );

        let attacking_hand = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::HandFront);
        let attacking_forearm =
            part_transform::<PlayerFighter>(&mut app, CutoutPartKind::ForearmFront);
        assert_ne!(
            attacking_hand.translation, neutral_hand.translation,
            "hand pivots into the attack pose"
        );
        assert_ne!(
            attacking_forearm.rotation, neutral_forearm.rotation,
            "forearm rotates into the attack pose"
        );
    }

    #[test]
    fn the_attack_envelope_winds_up_back_before_striking_forward() {
        let mut app = test_app();
        let rest_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::HandFront)
            .translation
            .x;
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);

        // Mid-anticipation: the striking hand pulls back behind its rest x.
        advance(&mut app, ATTACK_ANTICIPATION_SECONDS * 0.5);
        let windup_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::HandFront)
            .translation
            .x;
        assert!(
            windup_x < rest_x - 3.0,
            "anticipation winds the hand back: {windup_x} vs rest {rest_x}"
        );

        // End of the impact phase: full strike extension, exactly when the
        // lunge peaks (0.2 s = half the 0.4 s attack clip).
        advance(
            &mut app,
            ATTACK_ANTICIPATION_SECONDS * 0.5 + ATTACK_IMPACT_SECONDS,
        );
        let strike_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::HandFront)
            .translation
            .x;
        assert!(
            strike_x > rest_x + 10.0,
            "impact extends the hand forward: {strike_x} vs rest {rest_x}"
        );

        // Recovery releases back to idle by the end of the attack clip.
        advance(&mut app, 0.15);
        advance(&mut app, 0.15);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Idle);
    }

    #[test]
    fn the_attack_envelope_spans_exactly_the_attack_clip_and_lunge() {
        let envelope = PoseEnvelope::for_pose(CutoutPose::Attack).expect("attack has an envelope");
        assert_eq!(
            envelope.timer.duration(),
            FighterClip::Attack.animation().clip_duration(),
            "pose envelope, attack clip, and lunge all share one duration"
        );
        assert!(
            (envelope.anticipation + envelope.impact
                - envelope.timer.duration().as_secs_f32() / 2.0)
                .abs()
                < 1e-6,
            "the impact phase ends exactly at the lunge peak (progress 0.5)"
        );
    }

    #[test]
    fn the_hurt_envelope_overshoots_the_recoil_then_settles() {
        let mut app = test_app();
        let rest_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso)
            .translation
            .x;
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 4 });

        // The landed hit first freezes the defender for the hit-stop
        // window; the recoil envelope only starts ticking once it lifts.
        advance(&mut app, crate::arena::fx::HIT_STOP_SECONDS);
        advance(&mut app, 0.001);
        advance(&mut app, HURT_ANTICIPATION_SECONDS);
        let overshoot_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso)
            .translation
            .x;
        advance(&mut app, HURT_IMPACT_SECONDS);
        let settled_x = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso)
            .translation
            .x;
        assert!(
            overshoot_x < settled_x && settled_x < rest_x,
            "the recoil snaps past the held key then eases back onto it: \
             overshoot {overshoot_x}, settled {settled_x}, rest {rest_x}"
        );
    }

    #[test]
    fn a_landed_hit_freezes_only_the_struck_fighters_presentation_briefly() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 4 });

        // Inside the hit-stop window: the struck player's reaction pose is
        // frozen at rest while the attacking enemy's envelope advances.
        advance(&mut app, 0.03);
        let frozen_a = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        let attacker_a = part_transform::<EnemyFighter>(&mut app, CutoutPartKind::HandFront);
        advance(&mut app, 0.03);
        let frozen_b = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        let attacker_b = part_transform::<EnemyFighter>(&mut app, CutoutPartKind::HandFront);
        assert_eq!(
            frozen_a, frozen_b,
            "the struck fighter's pose does not progress during the hit-stop"
        );
        assert_ne!(
            attacker_a, attacker_b,
            "the attacker's envelope keeps playing through the defender's hit-stop"
        );

        // After the window the freeze lifts and the recoil plays.
        advance(&mut app, crate::arena::fx::HIT_STOP_SECONDS);
        advance(&mut app, 0.05);
        let recoiling = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        assert_ne!(frozen_b, recoiling, "the recoil starts once the stop ends");
    }

    #[test]
    fn misses_and_blocks_do_not_hit_stop_the_defender() {
        for event in [CombatEvent::Missed, CombatEvent::Blocked { dmg: 2 }] {
            let mut app = test_app();
            write_event(&mut app, CombatSide::Enemy, event);
            let stopped = app
                .world_mut()
                .query_filtered::<(), (With<crate::arena::fx::HitStop>, With<PlayerFighter>)>()
                .iter(app.world())
                .count();
            assert_eq!(stopped, 0, "{event:?} must not freeze the defender");
        }
    }

    #[test]
    fn a_knockdown_staggers_then_falls_and_holds_the_ground_sprawl() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Defeated);

        // Past the whole envelope: the sprawl holds instead of returning to
        // idle, and the pose stays bit-identical from then on.
        advance(&mut app, 0.3);
        advance(&mut app, 0.3);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Knockdown);
        let held = part_transform::<EnemyFighter>(&mut app, CutoutPartKind::Torso);
        advance(&mut app, 0.5);
        assert_eq!(rig_pose::<EnemyFighter>(&mut app), CutoutPose::Knockdown);
        assert_eq!(
            part_transform::<EnemyFighter>(&mut app, CutoutPartKind::Torso),
            held,
            "the finished knockdown envelope holds the sprawl exactly"
        );
    }

    #[test]
    fn reduced_motion_pins_poses_at_the_full_key_with_identical_timing() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 4 });

        // Immediately at the full key: no blended in-between frames.
        advance(&mut app, 0.001);
        let early = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        advance(
            &mut app,
            HURT_ANTICIPATION_SECONDS + HURT_IMPACT_SECONDS / 2.0,
        );
        let mid = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        assert_eq!(
            early, mid,
            "reduced motion never eases through intermediate pose frames"
        );

        // The envelope still times out on the exact same clock and returns
        // the pose to idle.
        advance(&mut app, HURT_TOTAL_SECONDS / 2.0 + 0.05);
        advance(&mut app, HURT_TOTAL_SECONDS / 2.0 + 0.05);
        assert_eq!(rig_pose::<PlayerFighter>(&mut app), CutoutPose::Idle);
    }

    #[test]
    fn idle_fighters_breathe_only_under_full_motion_and_legs_stay_planted() {
        let mut app = test_app();
        advance(&mut app, 0.4);
        let torso_a = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        let foot_a = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::FootFront);
        advance(&mut app, 0.9);
        let torso_b = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        let foot_b = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::FootFront);
        assert_ne!(
            torso_a.translation.y, torso_b.translation.y,
            "the idle torso breathes between two distinct sway phases"
        );
        assert_eq!(foot_a, foot_b, "feet stay planted through the sway");

        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        advance(&mut app, 0.4);
        let torso_a = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        advance(&mut app, 0.9);
        let torso_b = part_transform::<PlayerFighter>(&mut app, CutoutPartKind::Torso);
        assert_eq!(
            torso_a, torso_b,
            "reduced motion holds the idle rig perfectly still"
        );
    }

    #[test]
    fn a_landed_hit_plays_the_defenders_hurt() {
        let mut app = test_app();
        for event in [
            CombatEvent::Hit { dmg: 3 },
            CombatEvent::Crit { dmg: 6 },
            CombatEvent::Blocked { dmg: 1 },
        ] {
            let mut app2 = test_app();
            write_event(&mut app2, CombatSide::Enemy, event);
            assert_eq!(
                side_state::<PlayerFighter>(&mut app2).0,
                FighterClip::Hurt,
                "{event:?} hurts the player"
            );
            assert_eq!(side_state::<EnemyFighter>(&mut app2).0, FighterClip::Attack);
        }
        // Non-strike events change nothing.
        for event in [
            CombatEvent::Guarded,
            CombatEvent::Rested { amount: 5 },
            CombatEvent::OutOfStamina,
        ] {
            write_event(&mut app, CombatSide::Player, event);
            assert_eq!(side_state::<PlayerFighter>(&mut app).0, FighterClip::Idle);
        }
    }

    #[test]
    fn a_finished_once_clip_returns_to_idle() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 3 });
        assert_eq!(side_state::<PlayerFighter>(&mut app).0, FighterClip::Hurt);
        finish_animation::<PlayerFighter>(&mut app);
        app.update();
        let (clip, index) = side_state::<PlayerFighter>(&mut app);
        assert_eq!(clip, FighterClip::Idle, "hurt returns to idle");
        assert_eq!(index, FighterClip::Idle.animation().first);
    }

    #[test]
    fn a_defeat_plays_ko_and_freezes_on_the_last_frame() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Hit { dmg: 30 });
        write_event(&mut app, CombatSide::Player, CombatEvent::Defeated);
        assert_eq!(side_state::<EnemyFighter>(&mut app).0, FighterClip::Ko);
        finish_animation::<EnemyFighter>(&mut app);
        app.update();
        app.update();
        let (clip, index) = side_state::<EnemyFighter>(&mut app);
        assert_eq!(clip, FighterClip::Ko, "KO never returns to idle");
        assert_eq!(index, FighterClip::Ko.animation().last, "frozen last frame");
    }

    #[test]
    fn a_hit_on_a_downed_fighter_never_replaces_ko_with_hurt() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Defeated);
        write_event(&mut app, CombatSide::Player, CombatEvent::Hit { dmg: 3 });
        assert_eq!(side_state::<EnemyFighter>(&mut app).0, FighterClip::Ko);
    }

    /// Drives the marked fighter's animation to completion with explicit,
    /// deterministic deltas.
    fn finish_animation<M: Component>(app: &mut App) {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&mut SpriteAnimation, With<M>>();
        let mut anim = query.single_mut(world).expect("fighter exists");
        for _ in 0..16 {
            let current = anim.current_frame();
            anim.advance(ms(200), current);
        }
        assert!(anim.is_finished(), "animation ran to completion");
    }

    #[test]
    fn the_lunge_moves_the_attacker_out_and_ends_on_the_staged_x() {
        // Pure-logic pass over the ECS pieces: build a lunge, tick it midway
        // and to the end through the component API.
        let staging = ArenaStaging::starting();
        let mut lunge = AttackLunge::for_side(CombatSide::Player);
        let half = lunge.timer.duration() / 2;
        lunge.timer.tick(half);
        let mid = lunge_x(
            staging.x_of(lunge.side),
            staging.x_of(lunge.side.opponent()),
            lunge.timer.fraction(),
            false,
        );
        assert!(mid > staging.player_x, "moved towards the enemy");
        lunge.timer.tick(half);
        assert!(lunge.timer.is_finished(), "lunge ends with the attack clip");
    }

    #[test]
    fn footwork_eases_out_from_the_old_staged_x_to_the_new_one() {
        let (from, to) = (-30.0, -140.0);
        assert_eq!(footwork_x(from, to, 0.0, false), from);
        assert_eq!(
            footwork_x(from, to, 1.0, false),
            to,
            "lands exactly on the new x"
        );

        let quarter = footwork_x(from, to, 0.25, false);
        let half = footwork_x(from, to, 0.5, false);
        let three_quarters = footwork_x(from, to, 0.75, false);
        assert!(
            from > quarter && quarter > half && half > three_quarters && three_quarters > to,
            "the tween moves monotonically towards the new x, never back"
        );
        assert!(
            (half - from).abs() > (to - from).abs() / 2.0,
            "ease-out covers more than half the distance by the midpoint"
        );
    }

    #[test]
    fn reduced_motion_footwork_snaps_near_instantly_to_the_new_staged_x() {
        // Position is semantic state (#200): reduced motion never shortens
        // the step -- it lands the fighter on the new x immediately.
        let (from, to) = (-30.0, -140.0);
        for progress in [0.0, 0.1, 0.5, 1.0] {
            assert_eq!(footwork_x(from, to, progress, true), to);
        }
    }

    /// The player fighter's staged x from the app's [`ArenaStaging`].
    fn staged_player_x(app: &App) -> f32 {
        app.world().resource::<ArenaStaging>().player_x
    }

    /// Advances well past one [`FOOTWORK_DURATION`] in steps below virtual
    /// time's default `max_delta` clamp (0.25 s), so the whole duration
    /// actually elapses instead of being capped to one clamped frame.
    fn advance_past_footwork(app: &mut App) {
        let step = FOOTWORK_DURATION.as_secs_f32() / 2.0 + 0.01;
        advance(app, step);
        advance(app, step);
    }

    /// The enemy fighter's current transform x.
    fn enemy_transform_x(app: &mut App) -> f32 {
        app.world_mut()
            .query_filtered::<&Transform, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists")
            .translation
            .x
    }

    #[test]
    fn a_movement_event_tweens_only_the_actor_to_its_new_staged_x() {
        let mut app = test_app();
        let start = ArenaStaging::starting();
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            action: crate::combat::CombatAction::StepBack,
            event: CombatEvent::Moved {
                from: DuelDistance::CLOSE,
                to: DuelDistance::NEAR,
            },
        });
        app.update();
        advance_past_footwork(&mut app);

        assert_eq!(
            player_transform_x(&mut app),
            start.enemy_x - NEAR_GAP,
            "the actor lands exactly gap(to) from the standing opponent"
        );
        assert_eq!(
            enemy_transform_x(&mut app),
            start.enemy_x,
            "the standing opponent never moves without a wall hit"
        );
        assert_ne!(
            player_transform_x(&mut app),
            start.player_x,
            "the fighter does not return to where it stood before"
        );
    }

    #[test]
    fn a_wall_hit_slides_both_fighters_keeping_the_gap_exact() {
        let mut app = test_app();
        // close -> near: player retreats to 110 - 250 = -140.
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            action: crate::combat::CombatAction::StepBack,
            event: CombatEvent::Moved {
                from: DuelDistance::CLOSE,
                to: DuelDistance::NEAR,
            },
        });
        app.update();
        advance_past_footwork(&mut app);
        // near -> far: the raw target 110 - 360 = -250 crosses the left
        // wall; the residual slides the pair right together.
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            action: crate::combat::CombatAction::StepBack,
            event: CombatEvent::Moved {
                from: DuelDistance::NEAR,
                to: DuelDistance::FAR,
            },
        });
        app.update();
        advance_past_footwork(&mut app);

        let player_x = player_transform_x(&mut app);
        let enemy_x = enemy_transform_x(&mut app);
        assert_eq!(player_x, crate::arena::staging::STAGE_MIN_X);
        assert_eq!(enemy_x - player_x, FAR_GAP, "the gap stays exact");
        let staging = *app.world().resource::<ArenaStaging>();
        assert_eq!(
            (player_x, enemy_x),
            (staging.player_x, staging.enemy_x),
            "both transforms settle exactly on the staged positions"
        );
    }

    #[test]
    fn movement_events_play_the_actor_footwork_clip() {
        let mut app = test_app();
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            action: crate::combat::CombatAction::StepForward,
            event: CombatEvent::Moved {
                from: crate::combat::DuelDistance::NEAR,
                to: crate::combat::DuelDistance::CLOSE,
            },
        });
        app.update();

        let (clip, index) = side_state::<PlayerFighter>(&mut app);
        assert_eq!(clip, FighterClip::StepForward);
        assert_eq!(index, FighterClip::StepForward.animation().first);
        let footwork = app
            .world_mut()
            .query_filtered::<(), (With<FootworkStep>, With<PlayerFighter>)>()
            .iter(app.world())
            .count();
        assert_eq!(footwork, 1, "the actor gets a footwork tween");
    }

    #[test]
    fn reduced_motion_footwork_still_lands_the_fighter_on_the_new_staged_x() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            action: crate::combat::CombatAction::StepBack,
            event: CombatEvent::Moved {
                from: DuelDistance::CLOSE,
                to: DuelDistance::NEAR,
            },
        });
        app.update();
        let staged_x = staged_player_x(&app);
        assert_ne!(
            staged_x,
            ArenaStaging::starting().player_x,
            "the staged position itself moved -- position is semantic state"
        );
        advance(&mut app, 0.001);
        assert_eq!(
            player_transform_x(&mut app),
            staged_x,
            "reduced motion snaps near-instantly to the new staged x"
        );

        advance_past_footwork(&mut app);
        assert_eq!(
            player_transform_x(&mut app),
            staged_x,
            "the fighter stays on the new staged x after the tween window"
        );
    }

    #[test]
    fn backward_movement_events_play_the_back_step_clip() {
        let mut app = test_app();
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Enemy,
            action: crate::combat::CombatAction::StepBack,
            event: CombatEvent::Moved {
                from: crate::combat::DuelDistance::CLOSE,
                to: crate::combat::DuelDistance::NEAR,
            },
        });
        app.update();

        assert_eq!(
            side_state::<EnemyFighter>(&mut app).0,
            FighterClip::StepBack
        );
    }

    #[test]
    fn every_roster_rung_has_its_own_sheet_and_wraps_across_laps() {
        let sheets = FighterSpriteSheets::default();
        assert_eq!(OPPONENT_SHEETS.len(), crate::roster::LADDER.len());
        let unique: std::collections::HashSet<&str> = OPPONENT_SHEETS.iter().copied().collect();
        assert_eq!(unique.len(), OPPONENT_SHEETS.len(), "all sheets distinct");
        assert_eq!(
            sheets.opponent(0),
            sheets.opponent(10),
            "lap 2 reuses the lap 1 sheet"
        );
    }

    #[test]
    fn without_an_asset_server_the_sheets_count_as_ready() {
        let sheets = FighterSpriteSheets::default();
        assert!(sheets.ready(None), "headless apps never wait on assets");
    }

    /// Ignored art-iteration helper (the phase-4 `dump_template_geometry`
    /// pattern): dumps the *posed* part transforms of the player rig at
    /// interesting envelope moments as JSON, so external compositor tooling
    /// can render pose previews from the real runtime part art. Run with:
    /// `POSE_DUMP_DIR=/tmp/dump cargo test --lib \
    ///  arena::animation::tests::dump_posed_geometry -- --ignored`
    #[test]
    #[ignore]
    fn dump_posed_geometry() {
        let Ok(dir) = std::env::var("POSE_DUMP_DIR") else {
            return;
        };
        std::fs::create_dir_all(&dir).unwrap();
        let cases: Vec<(&str, Option<CombatEvent>, f32)> = vec![
            ("idle", None, 1.0),
            (
                "attack_windup",
                Some(CombatEvent::Missed),
                ATTACK_ANTICIPATION_SECONDS,
            ),
            (
                "attack_impact",
                Some(CombatEvent::Missed),
                ATTACK_ANTICIPATION_SECONDS + ATTACK_IMPACT_SECONDS,
            ),
            (
                "hurt_overshoot",
                Some(CombatEvent::Hit { dmg: 4 }),
                HURT_ANTICIPATION_SECONDS,
            ),
            ("block", Some(CombatEvent::Blocked { dmg: 2 }), 0.2),
            (
                "knockdown_stagger",
                Some(CombatEvent::Defeated),
                KNOCKDOWN_ANTICIPATION_SECONDS,
            ),
            ("knockdown_hold", Some(CombatEvent::Defeated), 1.0),
        ];
        for (name, event, at) in cases {
            let mut app = test_app();
            match event {
                // Hurt/block/knockdown read on the defender: strike as the
                // enemy so the *player* rig (unflipped) shows the reaction.
                Some(event @ (CombatEvent::Hit { .. } | CombatEvent::Blocked { .. })) => {
                    write_event(&mut app, CombatSide::Enemy, event);
                }
                Some(CombatEvent::Defeated) => {
                    write_event(&mut app, CombatSide::Enemy, CombatEvent::Defeated);
                }
                Some(event) => write_event(&mut app, CombatSide::Player, event),
                None => {}
            }
            // Split the advance so no step exceeds virtual time's clamp.
            advance(&mut app, at / 2.0);
            advance(&mut app, at / 2.0);

            let world = app.world_mut();
            let parent_kind_of: std::collections::HashMap<Entity, CutoutPartKind> = world
                .query::<(Entity, &CutoutPartMarker)>()
                .iter(world)
                .map(|(entity, marker)| (entity, marker.kind))
                .collect();
            let player = world
                .query_filtered::<Entity, With<PlayerFighter>>()
                .single(world)
                .expect("player exists");
            let parent_of: std::collections::HashMap<Entity, Entity> = world
                .query_filtered::<(Entity, &ChildOf), With<CutoutPartMarker>>()
                .iter(world)
                .map(|(entity, child_of)| (entity, child_of.parent()))
                .collect();
            let mut parts: Vec<serde_json::Value> = Vec::new();
            let mut query =
                world.query::<(Entity, &CutoutPartMarker, &CutoutPartRestPose, &Transform)>();
            for (entity, marker, rest, transform) in query.iter(world) {
                if cutout_rig_owner(entity, |e| parent_of.get(&e).copied()) != player {
                    continue;
                }
                let parent = parent_of
                    .get(&entity)
                    .and_then(|parent| parent_kind_of.get(parent))
                    .map(|kind| format!("{kind:?}"));
                parts.push(serde_json::json!({
                    "kind": format!("{:?}", marker.kind),
                    "parent": parent,
                    "offset": [transform.translation.x, transform.translation.y],
                    "rotation": transform.rotation.to_euler(EulerRot::XYZ).2,
                    "z_offset": transform.translation.z,
                    "size": [rest.size.x, rest.size.y],
                }));
            }
            std::fs::write(
                format!("{dir}/{name}.json"),
                serde_json::to_string_pretty(&parts).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn fighters_despawn_with_the_arena_but_animations_never_panic_after() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        let fighters = app
            .world_mut()
            .query_filtered::<(), With<Fighter>>()
            .iter(app.world())
            .count();
        assert_eq!(fighters, 0);
        app.update(); // animation systems tolerate an empty arena
    }
}
