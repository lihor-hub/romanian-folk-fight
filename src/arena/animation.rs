//! Sprite animation for the arena fighters (#22): the [`SpriteAnimation`]
//! timer component driving a `TextureAtlas` index, the per-fighter clip
//! table ([`FighterClip`]), the sprite-sheet handles and readiness guard
//! ([`FighterSpriteSheets`]), and the wiring that turns [`CombatLogEvent`]s
//! into attack / hurt / KO / footwork animations plus the attack lunge and
//! presentation-only footwork between the arena anchors.

use std::time::Duration;

use bevy::prelude::*;

use crate::character::{EnemyFighter, PlayerFighter};
use crate::combat::{CombatAction, CombatEvent, CombatLogEvent, CombatSide};
use crate::cutout::{CutoutPartMarker, CutoutPartRestPose, CutoutPose, CutoutRig};
use crate::roster::LADDER;

use super::{ENEMY_ANCHOR, PLAYER_ANCHOR};

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

/// How far towards the opponent's anchor the attack lunge peaks, as a
/// fraction of the distance between the two anchors.
const LUNGE_FRACTION: f32 = 0.35;

/// Presentation-only horizontal distance of a footwork step, in world units.
const FOOTWORK_DISTANCE: f32 = 28.0;

/// Duration of a footwork step-in or step-back tween.
const FOOTWORK_DURATION: Duration = Duration::from_millis(500);

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

/// Attack lunge of one fighter: an out-and-back tween from its own anchor
/// towards the opponent's, lasting exactly one attack clip.
#[derive(Component, Debug, Clone)]
pub struct AttackLunge {
    from: Vec3,
    toward: Vec3,
    timer: Timer,
}

impl AttackLunge {
    /// A lunge between the two anchors of `side`, timed to the attack clip.
    fn for_side(side: CombatSide) -> Self {
        let (from, toward) = match side {
            CombatSide::Player => (PLAYER_ANCHOR.translation, ENEMY_ANCHOR.translation),
            CombatSide::Enemy => (ENEMY_ANCHOR.translation, PLAYER_ANCHOR.translation),
        };
        Self {
            from,
            toward,
            timer: Timer::new(
                FighterClip::Attack.animation().clip_duration(),
                TimerMode::Once,
            ),
        }
    }
}

/// Position of a lunging fighter at `progress` in `0..=1`: an out-and-back
/// arc peaking at [`LUNGE_FRACTION`] of the way to the opponent's anchor.
pub fn lunge_position(from: Vec3, toward: Vec3, progress: f32) -> Vec3 {
    // `sin(PI)` is a hair negative in f32; the clamp keeps the endpoints
    // exactly on the anchor.
    let arc = (progress.clamp(0.0, 1.0) * std::f32::consts::PI)
        .sin()
        .max(0.0);
    let mut position = from + (toward - from) * (LUNGE_FRACTION * arc);
    position.z = from.z;
    position
}

/// Presentation-only footwork step: a short out-and-back horizontal motion
/// around the fighter's anchor. Combat distance changes live in the engine;
/// this component only makes them readable.
#[derive(Component, Debug, Clone)]
struct FootworkStep {
    anchor: Vec3,
    direction: f32,
    timer: Timer,
}

impl FootworkStep {
    /// Footwork for `side` and movement clip. Forward always means towards
    /// the opponent; backward means away, so the enemy side mirrors the x
    /// direction.
    fn for_side(side: CombatSide, clip: FighterClip) -> Self {
        let anchor = match side {
            CombatSide::Player => PLAYER_ANCHOR.translation,
            CombatSide::Enemy => ENEMY_ANCHOR.translation,
        };
        let side_forward = match side {
            CombatSide::Player => 1.0,
            CombatSide::Enemy => -1.0,
        };
        let direction = match clip {
            FighterClip::StepForward => side_forward,
            FighterClip::StepBack => -side_forward,
            _ => side_forward,
        };
        Self {
            anchor,
            direction,
            timer: Timer::new(FOOTWORK_DURATION, TimerMode::Once),
        }
    }

    fn position(&self) -> Vec3 {
        footwork_position(self.anchor, self.direction, self.timer.fraction())
    }
}

/// Position of a fighter during footwork at `progress` in `0..=1`.
/// Movement eases out to [`FOOTWORK_DISTANCE`] at the midpoint, then returns
/// to the exact anchor by the end.
fn footwork_position(anchor: Vec3, direction: f32, progress: f32) -> Vec3 {
    let progress = progress.clamp(0.0, 1.0);
    let leg = if progress <= 0.5 {
        progress * 2.0
    } else {
        (1.0 - progress) * 2.0
    };
    let eased = (leg * std::f32::consts::FRAC_PI_2).sin();
    let mut position = anchor;
    position.x += FOOTWORK_DISTANCE * direction.signum() * eased;
    position
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

/// Timer for a non-idle [`CutoutPose`] that should return to idle once its
/// presentation beat has read. Knockdowns intentionally do not carry one.
#[derive(Component, Debug, Clone)]
struct CutoutPoseTimer(Timer);

fn set_cutout_pose(
    commands: &mut Commands,
    entity: Entity,
    pose: CutoutPose,
    slot: &mut CutoutPose,
) {
    *slot = pose;
    match pose {
        CutoutPose::Idle | CutoutPose::Knockdown => {
            commands.entity(entity).remove::<CutoutPoseTimer>();
        }
        CutoutPose::Attack => {
            commands.entity(entity).insert(CutoutPoseTimer(Timer::new(
                FighterClip::Attack.animation().clip_duration(),
                TimerMode::Once,
            )));
        }
        CutoutPose::HitReaction => {
            commands.entity(entity).insert(CutoutPoseTimer(Timer::new(
                FighterClip::Hurt.animation().clip_duration(),
                TimerMode::Once,
            )));
        }
        CutoutPose::StepForward | CutoutPose::StepBack => {
            commands.entity(entity).insert(CutoutPoseTimer(Timer::new(
                FOOTWORK_DURATION,
                TimerMode::Once,
            )));
        }
        CutoutPose::Block | CutoutPose::Dodge => {
            commands.entity(entity).insert(CutoutPoseTimer(Timer::new(
                RIG_REACTION_DURATION,
                TimerMode::Once,
            )));
        }
    }
}

/// Maps this frame's combat events onto clips: any strike attempt plays the
/// attacker's attack (with a lunge), miss/reach failures make the defender
/// avoid, Hit/Crit/Blocked plays the defender's reaction, and Defeated plays
/// the defender's KO (which then freezes).
fn animate_combat_events(
    mut commands: Commands,
    mut events: MessageReader<CombatLogEvent>,
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
            CombatEvent::Moved { .. } => {
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
                        .insert(FootworkStep::for_side(actor, clip));
                }
            }
        }
    }
}

/// Returns timed rig-only poses to idle. The sprite-sheet clip system remains
/// authoritative for root clip state; this only clears jointed body poses.
fn tick_cutout_pose_timers(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut CutoutPose, &mut CutoutPoseTimer)>,
) {
    for (entity, mut pose, mut timer) in &mut query {
        timer.0.tick(time.delta());
        if timer.0.is_finished() {
            *pose = CutoutPose::Idle;
            commands.entity(entity).remove::<CutoutPoseTimer>();
        }
    }
}

/// Ticks every [`SpriteAnimation`] and writes the advanced frame into the
/// sprite's atlas index.
fn advance_animations(
    time: Res<Time>,
    mut query: Query<(&mut SpriteAnimation, Option<&mut Sprite>)>,
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

/// Applies the current jointed pose to every body-part child, rebuilding from
/// the part's neutral transform so gear parented beneath hands/arms/shields
/// inherits the same motion without independent drift.
fn apply_cutout_poses(
    fighters: Query<(&CutoutPose, Option<&CutoutRig>)>,
    mut parts: Query<(
        &CutoutPartMarker,
        &ChildOf,
        &CutoutPartRestPose,
        &mut Transform,
    )>,
) {
    for (marker, child_of, rest, mut transform) in &mut parts {
        let Ok((pose, rig)) = fighters.get(child_of.parent()) else {
            continue;
        };
        let flip_x = rig.map(|rig| rig.flip_x).unwrap_or(false);
        *transform = posed_part_transform(marker.kind, rest, *pose, flip_x);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct JointedPartDelta {
    offset: Vec2,
    rotation: f32,
}

fn posed_part_transform(
    kind: crate::cutout::CutoutPartKind,
    rest: &CutoutPartRestPose,
    pose: CutoutPose,
    flip_x: bool,
) -> Transform {
    let mut delta = jointed_part_delta(kind, pose);
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
        CutoutPose::Knockdown => match kind {
            Torso => (-24.0, -60.0, 1.22),
            Head => (-49.0, -57.0, 1.1),
            Hair => (-52.0, -56.0, 1.1),
            UpperArmFront => (-20.0, -52.0, 1.45),
            ForearmFront => (-36.0, -51.0, 1.7),
            HandFront => (-48.0, -49.0, 1.72),
            UpperArmBack => (-7.0, -60.0, 0.86),
            ForearmBack => (-16.0, -72.0, 1.08),
            HandBack => (-26.0, -80.0, 1.08),
            ThighFront => (19.0, -44.0, 1.0),
            ShinFront => (35.0, -42.0, 1.16),
            FootFront => (49.0, -38.0, 1.08),
            ThighBack => (-7.0, -49.0, 0.74),
            ShinBack => (4.0, -54.0, 0.64),
            FootBack => (15.0, -55.0, 0.54),
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

/// Tweens lunging fighters along [`lunge_position`] and snaps them back to
/// their anchor when the lunge ends.
fn apply_lunges(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut AttackLunge, &mut Transform)>,
) {
    for (entity, mut lunge, mut transform) in &mut query {
        lunge.timer.tick(time.delta());
        if lunge.timer.is_finished() {
            transform.translation = lunge.from;
            commands.entity(entity).remove::<AttackLunge>();
        } else {
            transform.translation =
                lunge_position(lunge.from, lunge.toward, lunge.timer.fraction());
        }
    }
}

/// Applies presentation-only footwork and snaps fighters exactly back to
/// their anchors at the end.
fn apply_footwork(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut FootworkStep, &mut Transform)>,
) {
    for (entity, mut footwork, mut transform) in &mut query {
        footwork.timer.tick(time.delta());
        if footwork.timer.is_finished() {
            transform.translation = footwork.anchor;
            commands.entity(entity).remove::<FootworkStep>();
        } else {
            transform.translation = footwork.position();
        }
    }
}

/// Registers the sheet loading and the animation systems; added by the
/// arena plugin.
pub(super) struct AnimationPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum AnimationSet {
    Apply,
}

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        // Idempotent with CombatPlugin's registration; keeps the arena
        // usable in apps built without the combat plugin (tests).
        app.add_message::<CombatLogEvent>();
        app.add_systems(Startup, load_fighter_sheets).add_systems(
            Update,
            (
                animate_combat_events,
                advance_animations,
                return_to_idle,
                tick_cutout_pose_timers,
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
    use crate::character::{Attributes, Fighter};
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
    fn the_lunge_arcs_out_and_back_between_the_anchors() {
        let from = PLAYER_ANCHOR.translation;
        let toward = ENEMY_ANCHOR.translation;
        assert_eq!(lunge_position(from, toward, 0.0), from);
        assert_eq!(lunge_position(from, toward, 1.0), from);
        let peak = lunge_position(from, toward, 0.5);
        assert!(peak.x > from.x, "the player lunges rightwards");
        assert!(
            (peak.x - (from.x + (toward.x - from.x) * LUNGE_FRACTION)).abs() < 1e-3,
            "peaks at the lunge fraction"
        );
        assert_eq!(peak.z, from.z, "z never changes");
        let quarter = lunge_position(from, toward, 0.25);
        assert!(from.x < quarter.x && quarter.x < peak.x, "smooth arc out");
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

    fn part_transform<M: Component>(app: &mut App, kind: CutoutPartKind) -> Transform {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&CutoutPartMarker, &ChildOf, &Transform), ()>();
        query
            .iter(world)
            .find_map(|(marker, child_of, transform)| {
                let parent = child_of.parent();
                world
                    .get::<M>(parent)
                    .filter(|_| marker.kind == kind)
                    .map(|_| *transform)
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
    fn the_lunge_moves_the_attacker_out_and_snaps_back_on_its_anchor() {
        // Pure-logic pass over the ECS pieces: build a lunge, tick it midway
        // and to the end through the component API.
        let mut lunge = AttackLunge::for_side(CombatSide::Player);
        let half = lunge.timer.duration() / 2;
        lunge.timer.tick(half);
        let mid = lunge_position(lunge.from, lunge.toward, lunge.timer.fraction());
        assert!(
            mid.x > PLAYER_ANCHOR.translation.x,
            "moved towards the enemy"
        );
        lunge.timer.tick(half);
        assert!(lunge.timer.is_finished(), "lunge ends with the attack clip");
    }

    #[test]
    fn footwork_positions_ease_out_and_restore_the_anchor() {
        let anchor = PLAYER_ANCHOR.translation;
        assert_eq!(footwork_position(anchor, 1.0, 0.0), anchor);
        assert_eq!(footwork_position(anchor, 1.0, 1.0), anchor);

        let quarter = footwork_position(anchor, 1.0, 0.25);
        let half = footwork_position(anchor, 1.0, 0.5);
        let three_quarters = footwork_position(anchor, 1.0, 0.75);
        assert!(quarter.x > anchor.x, "forward footwork starts rightward");
        assert!(
            (half.x - (anchor.x + FOOTWORK_DISTANCE)).abs() < 1e-3,
            "midpoint reaches the configured step distance"
        );
        assert!(
            three_quarters.x > anchor.x && three_quarters.x < half.x,
            "the second half returns towards the anchor"
        );
        assert_eq!(half.z, anchor.z, "z never changes");
    }

    #[test]
    fn enemy_forward_footwork_mirrors_towards_the_player() {
        let mut player = FootworkStep::for_side(CombatSide::Player, FighterClip::StepForward);
        let mut enemy = FootworkStep::for_side(CombatSide::Enemy, FighterClip::StepForward);
        player.timer.tick(player.timer.duration() / 2);
        enemy.timer.tick(enemy.timer.duration() / 2);
        let player_mid = player.position();
        let enemy_mid = enemy.position();
        assert!(player_mid.x > PLAYER_ANCHOR.translation.x);
        assert!(enemy_mid.x < ENEMY_ANCHOR.translation.x);
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
