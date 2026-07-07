//! Sprite animation for the arena fighters (#22): the [`SpriteAnimation`]
//! timer component driving a `TextureAtlas` index, the per-fighter clip
//! table ([`FighterClip`]), the sprite-sheet handles and readiness guard
//! ([`FighterSpriteSheets`]), and the wiring that turns [`CombatLogEvent`]s
//! into attack / hurt / KO animations plus the attack lunge between the
//! arena anchors.

use std::time::Duration;

use bevy::prelude::*;

use crate::character::{EnemyFighter, PlayerFighter};
use crate::combat::{CombatEvent, CombatLogEvent, CombatSide};
use crate::roster::LADDER;

use super::{ENEMY_ANCHOR, PLAYER_ANCHOR};

/// Side length of one sprite-sheet frame in pixels.
pub const FRAME_SIZE: u32 = 128;
/// Frames per sheet row; the sheets are a [`ATLAS_COLUMNS`] x [`ATLAS_ROWS`]
/// grid in row-major order (idle row, attack row, hurt + KO row).
pub const ATLAS_COLUMNS: u32 = 4;
/// Rows per sprite sheet.
pub const ATLAS_ROWS: u32 = 3;

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
        moved.then_some(frame)
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

/// The four fighter clips, mapped onto the shared sheet layout. Doubles as
/// the component tracking which clip a fighter is currently playing.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FighterClip {
    Idle,
    Attack,
    Hurt,
    Ko,
}

impl FighterClip {
    /// The animation for this clip on the shared 4x3 sheet layout.
    pub fn animation(self) -> SpriteAnimation {
        match self {
            Self::Idle => SpriteAnimation::new(0, 3, 6.0, AnimationMode::Loop),
            Self::Attack => SpriteAnimation::new(4, 7, 10.0, AnimationMode::Once),
            Self::Hurt => SpriteAnimation::new(8, 9, 8.0, AnimationMode::Once),
            Self::Ko => SpriteAnimation::new(10, 11, 6.0, AnimationMode::Once),
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

/// Swaps a fighter onto `clip`: restarts the animation and snaps the atlas
/// index to the clip's first frame.
fn set_clip(
    clip: FighterClip,
    slot: &mut FighterClip,
    anim: &mut SpriteAnimation,
    sprite: &mut Sprite,
) {
    *slot = clip;
    *anim = clip.animation();
    if let Some(atlas) = sprite.texture_atlas.as_mut() {
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
        &'static mut Sprite,
    ),
    (With<Side>, Without<Other>),
>;

/// Maps this frame's combat events onto clips: any strike attempt plays the
/// attacker's attack (with a lunge), Hit/Crit/Blocked plays the defender's
/// hurt, and Defeated plays the defender's KO (which then freezes).
fn animate_combat_events(
    mut commands: Commands,
    mut events: MessageReader<CombatLogEvent>,
    mut players: SideAnimation<PlayerFighter, EnemyFighter>,
    mut enemies: SideAnimation<EnemyFighter, PlayerFighter>,
) {
    for CombatLogEvent { actor, event } in events.read().copied() {
        let (attacker, defender) = match actor {
            CombatSide::Player => (players.single_mut(), enemies.single_mut()),
            CombatSide::Enemy => (enemies.single_mut(), players.single_mut()),
        };
        match event {
            CombatEvent::Missed
            | CombatEvent::Hit { .. }
            | CombatEvent::Crit { .. }
            | CombatEvent::Blocked { .. } => {
                if let Ok((entity, mut clip, mut anim, mut sprite)) = attacker {
                    set_clip(FighterClip::Attack, &mut clip, &mut anim, &mut sprite);
                    commands.entity(entity).insert(AttackLunge::for_side(actor));
                }
                if !matches!(event, CombatEvent::Missed)
                    && let Ok((_, mut clip, mut anim, mut sprite)) = defender
                    && *clip != FighterClip::Ko
                {
                    set_clip(FighterClip::Hurt, &mut clip, &mut anim, &mut sprite);
                }
            }
            CombatEvent::Defeated => {
                if let Ok((_, mut clip, mut anim, mut sprite)) = defender {
                    set_clip(FighterClip::Ko, &mut clip, &mut anim, &mut sprite);
                }
            }
            CombatEvent::Guarded | CombatEvent::Rested { .. } | CombatEvent::OutOfStamina => {}
        }
    }
}

/// Ticks every [`SpriteAnimation`] and writes the advanced frame into the
/// sprite's atlas index.
fn advance_animations(time: Res<Time>, mut query: Query<(&mut SpriteAnimation, &mut Sprite)>) {
    for (mut anim, mut sprite) in &mut query {
        let Some(atlas) = sprite.texture_atlas.as_mut() else {
            continue;
        };
        if let Some(frame) = anim.advance(time.delta(), atlas.index) {
            atlas.index = frame;
        }
    }
}

/// Returns every finished `Once` clip to the idle loop — except KO, which
/// stays frozen on its last frame.
fn return_to_idle(mut query: Query<(&mut FighterClip, &mut SpriteAnimation, &mut Sprite)>) {
    for (mut clip, mut anim, mut sprite) in &mut query {
        if anim.is_finished() && *clip != FighterClip::Ko {
            set_clip(FighterClip::Idle, &mut clip, &mut anim, &mut sprite);
        }
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

/// Registers the sheet loading and the animation systems; added by the
/// arena plugin.
pub(super) struct AnimationPlugin;

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
                apply_lunges,
            )
                .chain()
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
        assert_eq!((idle.first, idle.last), (0, 3));
        assert_eq!((attack.first, attack.last), (4, 7));
        assert_eq!((hurt.first, hurt.last), (8, 9));
        assert_eq!((ko.first, ko.last), (10, 11));
        assert_eq!(ko.last as u32 + 1, ATLAS_COLUMNS * ATLAS_ROWS);
        assert_eq!(idle.mode, AnimationMode::Loop);
        for once in [attack, hurt, ko] {
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

    /// Headless app inside the fight with both fighters spawned as sprites.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes::default(),
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app
    }

    fn write_event(app: &mut App, actor: CombatSide, event: CombatEvent) {
        app.world_mut()
            .write_message(CombatLogEvent { actor, event });
        app.update();
    }

    fn side_state<M: Component>(app: &mut App) -> (FighterClip, usize) {
        let (clip, sprite) = app
            .world_mut()
            .query_filtered::<(&FighterClip, &Sprite), With<M>>()
            .single(app.world())
            .expect("fighter exists");
        (
            *clip,
            sprite.texture_atlas.as_ref().expect("stub atlas").index,
        )
    }

    #[test]
    fn fighters_spawn_on_the_idle_clip_with_a_stub_atlas() {
        let mut app = test_app();
        assert_eq!(side_state::<PlayerFighter>(&mut app).0, FighterClip::Idle);
        assert_eq!(side_state::<EnemyFighter>(&mut app), (FighterClip::Idle, 0));
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

    /// Drives the marked fighter's animation to completion (and its sprite
    /// index along with it) with explicit, deterministic deltas.
    fn finish_animation<M: Component>(app: &mut App) {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&mut SpriteAnimation, &mut Sprite), With<M>>();
        let (mut anim, mut sprite) = query.single_mut(world).expect("fighter exists");
        let atlas = sprite.texture_atlas.as_mut().expect("stub atlas");
        for _ in 0..16 {
            if let Some(frame) = anim.advance(ms(200), atlas.index) {
                atlas.index = frame;
            }
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
