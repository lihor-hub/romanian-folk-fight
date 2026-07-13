//! Arena presentation FX (#23): tiered parallax backgrounds, floating
//! damage numbers, sprite-based hit particles, and damage-scaled screen
//! shake.
//!
//! Everything spawned here is tagged with the arena's screen marker, so the
//! `OnExit(GameState::Fight)` cleanup catches any FX still alive; in normal
//! play every FX entity also despawns itself when its timer runs out.
//!
//! ## Non-color combat cues (#214)
//!
//! [`spawn_combat_fx`] and [`crate::combat::hud::collect_log_lines`] both
//! read the *same* `MessageReader<CombatLogEvent>` stream and `match` on the
//! same [`CombatEvent`] variants, so the floating cue and the combat-log
//! line for one event can never drift out of sync with each other — there
//! is only one classification of "what happened," reused by both. Each of
//! the four current outcomes is distinguishable without color:
//!
//! | Outcome | Text | Size | Particles |
//! | --- | --- | --- | --- |
//! | Hit | plain damage number (`"6"`) | [`DAMAGE_FONT_SIZE`] | [`HIT_PARTICLE_COUNT`] |
//! | Crit | damage number + [`CRIT_SUFFIX`] (`"12 CRITIC!"`) | [`CRIT_FONT_SIZE`] (bigger) | [`CRIT_PARTICLE_COUNT`] (more) |
//! | Blocked | [`BLOCKED_PREFIX`] + chip damage (`"Blocat 3"`) | [`DAMAGE_FONT_SIZE`] | [`BLOCK_PARTICLE_COUNT`] (few, gray) |
//! | Missed / OutOfReach | [`MISS_TEXT`] (`"Ratat!"`) | [`DAMAGE_FONT_SIZE`] | none |
//!
//! `CRITIC!`/`Blocat` reuse the combat log's existing Romanian vocabulary
//! (`log_line`'s "lovitură critică" / "blochează") rather than inventing new
//! wording.
//!
//! ### Prospective #150 contract (buff/debuff cues)
//!
//! No buff/debuff mechanics, icons, or lifecycle exist yet — #150 owns that
//! model and hasn't landed. This is the acceptance rule #150 (and any later
//! effect) must satisfy once it does, so its cues stay consistent with the
//! four above instead of being color-only:
//!
//! 1. **Same source.** The cue and its log line must both be built from the
//!    same event/effect classification — one `match`, read by both the FX
//!    spawner and the log formatter, exactly as `spawn_combat_fx` and
//!    `log_line` share [`CombatEvent`] here. No separate "cue-only" event
//!    type that could disagree with the log.
//! 2. **A text label**, reusing established announcer/log vocabulary (short
//!    Romanian words/phrases, not new invented terms), distinct from every
//!    other effect's label and from the four outcomes above.
//! 3. **A shape/size cue** distinct from the four outcomes above and from
//!    every other effect — its own particle count/pattern, icon, or size,
//!    not a recolored copy of hit/crit/block/miss.
//! 4. Color may still differ per effect, but color alone is never sufficient
//!    to tell two effects (or an effect and an outcome) apart.
//!
//! This slice adds no buff/debuff enum variants, constants, or systems —
//! only this contract for #150 to implement against.

use bevy::prelude::*;

use crate::character::{EnemyFighter, PlayerFighter};
use crate::combat::{CombatAction, CombatEvent, CombatLogEvent, CombatSide};
use crate::core::{GameState, UiFont};
use crate::roster::LadderProgress;
use crate::settings::AccessibilityPreferences;
use crate::theme::{BLOCKED_GRAY, CREAM, CRIT_GOLD};

use super::{ARENA_HEIGHT, ArenaScreen, FIGHTER_SIZE};

// --- Backgrounds -----------------------------------------------------------

/// The three arena themes over one ladder lap (per the #20 tiers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundTier {
    /// Fights 1–4: sat românesc (village square, fences, haystacks).
    Village,
    /// Fights 5–7: Pădurea întunecată (dark forest).
    Forest,
    /// Fights 8–10: Munții Carpați (mountain pass, fortress silhouette).
    Mountains,
}

impl BackgroundTier {
    /// Asset paths of this tier's `(far, near)` parallax layers.
    fn layer_paths(self) -> (&'static str, &'static str) {
        match self {
            Self::Village => (
                "backgrounds/village_far.png",
                "backgrounds/village_near.png",
            ),
            Self::Forest => ("backgrounds/forest_far.png", "backgrounds/forest_near.png"),
            Self::Mountains => (
                "backgrounds/mountains_far.png",
                "backgrounds/mountains_near.png",
            ),
        }
    }

    /// Asset path of this tier's stable stage-depth foreground layer.
    fn foreground_path(self) -> &'static str {
        match self {
            Self::Village => "backgrounds/village_foreground.png",
            Self::Forest => "backgrounds/forest_foreground.png",
            Self::Mountains => "backgrounds/mountains_foreground.png",
        }
    }
}

/// The tier backing the current ladder fight; laps wrap (fight 11 is back
/// in the village).
pub fn background_tier(ladder: LadderProgress) -> BackgroundTier {
    match ladder.0 % 10 {
        0..=3 => BackgroundTier::Village,
        4..=6 => BackgroundTier::Forest,
        _ => BackgroundTier::Mountains,
    }
}

/// Handles to every background layer, loaded at startup like the fighter
/// sprite sheets. Headless apps (no `AssetServer`) keep default handles.
#[derive(Resource, Default)]
pub struct ArenaBackgrounds {
    village: BackgroundHandles,
    forest: BackgroundHandles,
    mountains: BackgroundHandles,
}

#[derive(Default)]
struct BackgroundHandles {
    far: Handle<Image>,
    near: Handle<Image>,
    foreground: Handle<Image>,
}

impl ArenaBackgrounds {
    /// The `(far, near)` layer handles for `tier`.
    pub fn layers(&self, tier: BackgroundTier) -> (Handle<Image>, Handle<Image>) {
        let handles = self.handles(tier);
        (handles.far.clone(), handles.near.clone())
    }

    /// The stage-depth foreground handle for `tier`.
    fn foreground(&self, tier: BackgroundTier) -> Handle<Image> {
        self.handles(tier).foreground.clone()
    }

    fn handles(&self, tier: BackgroundTier) -> &BackgroundHandles {
        match tier {
            BackgroundTier::Village => &self.village,
            BackgroundTier::Forest => &self.forest,
            BackgroundTier::Mountains => &self.mountains,
        }
    }
}

fn load_backgrounds(mut commands: Commands, asset_server: Option<Res<AssetServer>>) {
    let mut backgrounds = ArenaBackgrounds::default();
    if let Some(server) = asset_server {
        for (tier, slot) in [
            (BackgroundTier::Village, &mut backgrounds.village),
            (BackgroundTier::Forest, &mut backgrounds.forest),
            (BackgroundTier::Mountains, &mut backgrounds.mountains),
        ] {
            let (far, near) = tier.layer_paths();
            *slot = BackgroundHandles {
                far: server.load(far),
                near: server.load(near),
                foreground: server.load(tier.foreground_path()),
            };
        }
    }
    commands.insert_resource(backgrounds);
}

/// A background layer drifting idly around `base_x`; `rate` scales the
/// shared drift so far and near layers move at different speeds.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ParallaxLayer {
    /// Parallax factor: how much of the drift this layer picks up.
    pub rate: f32,
    /// Resting x the drift oscillates around.
    pub base_x: f32,
}

/// Stable foreground/stage-depth layer for one background tier.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaForeground {
    /// Tier whose stage edge this entity displays.
    pub tier: BackgroundTier,
}

/// Parallax speed of the distant backdrop relative to the near overlay.
pub const FAR_LAYER_RATE: f32 = 0.35;
/// Parallax speed of the foreground overlay (the reference rate).
pub const NEAR_LAYER_RATE: f32 = 1.0;
/// Peak sideways drift of the near layer, in world units. The layers are
/// 900 px wide over the 800 px arena, so ±25 px never shows an edge.
const DRIFT_AMPLITUDE: f32 = 22.0;
/// Angular frequency of the idle drift, in rad/s — one slow sway ~40 s.
const DRIFT_FREQUENCY: f32 = 0.16;
/// Rendered size of one background layer (900x600 source pixels).
const LAYER_SIZE: Vec2 = Vec2::new(900.0, ARENA_HEIGHT);

/// Spawns the two parallax layers for the current tier. Called from the
/// arena's scene setup; both layers carry the screen marker and despawn
/// with the arena.
pub(super) fn spawn_background(
    commands: &mut Commands,
    backgrounds: &ArenaBackgrounds,
    tier: BackgroundTier,
) {
    let (far, near) = backgrounds.layers(tier);
    for (image, rate, z) in [(far, FAR_LAYER_RATE, -12.0), (near, NEAR_LAYER_RATE, -11.0)] {
        commands.spawn((
            ArenaScreen,
            ParallaxLayer { rate, base_x: 0.0 },
            Sprite {
                image,
                custom_size: Some(LAYER_SIZE),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, z),
        ));
    }
    commands.spawn((
        ArenaScreen,
        ArenaForeground { tier },
        Sprite {
            image: backgrounds.foreground(tier),
            custom_size: Some(LAYER_SIZE),
            ..default()
        },
        // Behind fighters and labels, but in front of the ground strip and
        // parallax art so the duel reads as staged without hiding silhouettes.
        Transform::from_xyz(0.0, 0.0, -8.5),
    ));
}

/// The subtle idle drift: each layer sways around its base at its own
/// parallax rate. Reduced motion (#200) holds every layer at its exact rest
/// `base_x` instead -- computed fresh every frame from `time`/the
/// preference (no drift phase or offset is ever stored on the component),
/// so flipping the preference mid-scene has nothing to "un-stick": the very
/// next frame either resumes the sway at whatever phase `time.elapsed_secs()`
/// is now at, or snaps straight to rest.
fn drift_parallax_layers(
    time: Res<Time>,
    accessibility: Res<AccessibilityPreferences>,
    mut layers: Query<(&ParallaxLayer, &mut Transform)>,
) {
    if accessibility.reduced_motion {
        for (layer, mut transform) in &mut layers {
            transform.translation.x = layer.base_x;
        }
        return;
    }
    let sway = (time.elapsed_secs() * DRIFT_FREQUENCY).sin() * DRIFT_AMPLITUDE;
    for (layer, mut transform) in &mut layers {
        transform.translation.x = layer.base_x + sway * layer.rate;
    }
}

// --- Floating damage numbers ------------------------------------------------

/// Lifetime of a floating damage number, in seconds.
pub const DAMAGE_TEXT_LIFETIME: f32 = 0.8;
/// Upward speed of a floating damage number, world units per second.
const DAMAGE_TEXT_RISE_SPEED: f32 = 70.0;
/// Font size of a regular hit / blocked / missed number.
pub const DAMAGE_FONT_SIZE: f32 = 26.0;
/// Font size of a crit number — visibly bigger.
pub const CRIT_FONT_SIZE: f32 = 40.0;
/// The floating text shown when a strike misses.
pub const MISS_TEXT: &str = "Ratat!";
/// Appended to a crit's damage number (#214 non-color cue), echoing the
/// combat log's "lovitură critică" wording.
pub const CRIT_SUFFIX: &str = "CRITIC!";
/// Prefixed to a block's chip-damage number (#214 non-color cue), echoing
/// the combat log's "blochează" wording.
pub const BLOCKED_PREFIX: &str = "Blocat";

/// A floating damage number: rises, fades, and despawns when the timer
/// runs out.
#[derive(Component)]
pub struct DamageText {
    timer: Timer,
}

/// Spawn height of a damage number above the defender's center.
const DAMAGE_TEXT_OFFSET_Y: f32 = FIGHTER_SIZE.y / 2.0 + 10.0;

fn spawn_damage_text(
    commands: &mut Commands,
    at: Vec3,
    text: String,
    size: f32,
    color: Color,
    ui_font: &UiFont,
) {
    commands.spawn((
        ArenaScreen,
        DamageText {
            timer: Timer::from_seconds(DAMAGE_TEXT_LIFETIME, TimerMode::Once),
        },
        Text2d::new(text),
        ui_font.text_font(size),
        TextColor(color),
        Transform::from_translation(Vec3::new(at.x, at.y + DAMAGE_TEXT_OFFSET_Y, 6.0)),
    ));
}

/// Rises and fades every damage number, despawning it at end of life.
fn animate_damage_text(
    time: Res<Time>,
    mut commands: Commands,
    mut texts: Query<(Entity, &mut DamageText, &mut Transform, &mut TextColor)>,
) {
    for (entity, mut text, mut transform, mut color) in &mut texts {
        text.timer.tick(time.delta());
        if text.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.y += DAMAGE_TEXT_RISE_SPEED * time.delta_secs();
        color.0.set_alpha(1.0 - text.timer.fraction());
    }
}

// --- Hit particles -----------------------------------------------------------

/// Sparks in a regular hit burst.
pub const HIT_PARTICLE_COUNT: usize = 8;
/// Sparks in a crit burst — visibly more.
pub const CRIT_PARTICLE_COUNT: usize = 20;
/// Chips in a block burst — visibly fewer than a hit, and (unlike a miss,
/// which bursts none) still present: a #214 non-color cue distinguishing
/// "blocked" from both "hit" and "missed" by particle count alone.
pub const BLOCK_PARTICLE_COUNT: usize = 3;
/// Lifetime of one spark, in seconds.
pub const PARTICLE_LIFETIME: f32 = 0.45;
/// Downward pull on sparks, world units per second squared.
const PARTICLE_GRAVITY: f32 = -640.0;
/// Side length of one square spark quad.
const PARTICLE_SIZE: f32 = 5.0;

/// One spark quad: flies on `velocity` under gravity, fades, and despawns
/// when the timer runs out.
#[derive(Component)]
pub struct HitParticle {
    velocity: Vec2,
    timer: Timer,
}

/// Bursts `count` sparks from `at`, fanned deterministically over the
/// upper arc.
fn spawn_particles(commands: &mut Commands, at: Vec3, count: usize, color: Color) {
    for i in 0..count {
        let angle = std::f32::consts::TAU * (i as f32 + 0.5) / count as f32;
        let speed = 140.0 + 60.0 * ((i % 3) as f32);
        commands.spawn((
            ArenaScreen,
            HitParticle {
                velocity: Vec2::new(angle.cos() * speed, angle.sin().abs() * speed + 60.0),
                timer: Timer::from_seconds(PARTICLE_LIFETIME, TimerMode::Once),
            },
            Sprite::from_color(color, Vec2::splat(PARTICLE_SIZE)),
            Transform::from_translation(Vec3::new(at.x, at.y, 5.0)),
        ));
    }
}

/// Integrates spark motion (velocity + gravity), fades them out, and
/// despawns each at end of life.
fn animate_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut particles: Query<(Entity, &mut HitParticle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform, mut sprite) in &mut particles {
        particle.timer.tick(time.delta());
        if particle.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y += PARTICLE_GRAVITY * dt;
        transform.translation.x += particle.velocity.x * dt;
        transform.translation.y += particle.velocity.y * dt;
        let fraction = particle.timer.fraction();
        sprite.color.set_alpha(1.0 - fraction);
    }
}

// --- Screen shake -------------------------------------------------------------

/// Duration of one shake, in seconds.
pub const SHAKE_DURATION: f32 = 0.25;

/// The active camera shake, if any. `rest` is the camera translation
/// captured when the shake began; the camera is restored to it exactly
/// when the timer runs out (or on leaving the fight).
#[derive(Resource, Default)]
pub struct ScreenShake {
    active: bool,
    timer: Timer,
    amplitude: f32,
    rest: Vec3,
}

impl ScreenShake {
    /// Whether a shake is currently displacing the camera.
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn trigger(&mut self, amplitude: f32, camera_rest: Vec3) {
        if !self.active {
            self.rest = camera_rest;
            self.active = true;
        }
        self.timer = Timer::from_seconds(SHAKE_DURATION, TimerMode::Once);
        self.amplitude = self.amplitude.max(amplitude);
    }
}

/// Shake amplitude for `dmg` points of damage, in world units.
fn shake_amplitude(dmg: i32) -> f32 {
    (dmg as f32 * 0.45).clamp(3.0, 14.0)
}

/// Applies decaying pseudo-noise to the camera while a shake is active and
/// restores the camera exactly to its rest translation when it ends.
///
/// Reduced motion (#200) still ticks `shake.timer` at the same rate and
/// deactivates it after exactly [`SHAKE_DURATION`] -- the shake's
/// bookkeeping (and thus every other system's `ScreenShake::is_active`
/// read) is identical either way, per the issue's timing invariant -- but
/// the camera itself is pinned to `shake.rest` for the whole duration
/// instead of receiving the noise offset. Because the camera transform is
/// recomputed from `shake` fresh every frame (never accumulated), flipping
/// the preference mid-shake immediately snaps the camera back to rest with
/// nothing left stuck off-center.
fn apply_screen_shake(
    time: Res<Time>,
    mut shake: ResMut<ScreenShake>,
    accessibility: Res<AccessibilityPreferences>,
    mut cameras: Query<&mut Transform, With<crate::core::WorldCamera>>,
) {
    if !shake.active {
        return;
    }
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };
    shake.timer.tick(time.delta());
    if shake.timer.is_finished() {
        camera.translation = shake.rest;
        shake.active = false;
        shake.amplitude = 0.0;
        return;
    }
    if accessibility.reduced_motion {
        camera.translation = shake.rest;
        return;
    }
    // Deterministic offset noise: two incommensurate sine taps, decaying
    // linearly over the shake.
    let t = shake.timer.elapsed_secs();
    let decay = 1.0 - shake.timer.fraction();
    let offset = Vec3::new(
        (t * 73.0).sin() * shake.amplitude * decay,
        (t * 97.0 + 1.7).sin() * shake.amplitude * decay,
        0.0,
    );
    camera.translation = shake.rest + offset;
}

/// `OnExit(Fight)` safety net: if the state flips mid-shake, put the camera
/// back at rest immediately.
fn reset_screen_shake(
    mut shake: ResMut<ScreenShake>,
    mut cameras: Query<&mut Transform, With<crate::core::WorldCamera>>,
) {
    if !shake.active {
        return;
    }
    if let Ok(mut camera) = cameras.single_mut() {
        camera.translation = shake.rest;
    }
    shake.active = false;
    shake.amplitude = 0.0;
}

// --- Event -> FX wiring ---------------------------------------------------------

/// Turns this frame's [`CombatLogEvent`]s into FX at the defender: floating
/// damage numbers, spark bursts, and screen shake on crits and heavy-strike
/// hits.
fn spawn_combat_fx(
    mut events: MessageReader<CombatLogEvent>,
    mut commands: Commands,
    mut shake: ResMut<ScreenShake>,
    players: Query<&Transform, (With<PlayerFighter>, Without<crate::core::WorldCamera>)>,
    enemies: Query<&Transform, (With<EnemyFighter>, Without<crate::core::WorldCamera>)>,
    cameras: Query<&Transform, With<crate::core::WorldCamera>>,
    ui_font: Res<UiFont>,
) {
    for CombatLogEvent {
        actor,
        action,
        event,
    } in events.read().copied()
    {
        let defender = match actor.opponent() {
            CombatSide::Player => players.single(),
            CombatSide::Enemy => enemies.single(),
        };
        let Ok(at) = defender.map(|transform| transform.translation) else {
            continue;
        };
        let camera_rest = cameras
            .single()
            .map(|camera| camera.translation)
            .unwrap_or(Vec3::ZERO);
        match event {
            CombatEvent::Hit { dmg } => {
                spawn_damage_text(
                    &mut commands,
                    at,
                    dmg.to_string(),
                    DAMAGE_FONT_SIZE,
                    CREAM,
                    &ui_font,
                );
                spawn_particles(&mut commands, at, HIT_PARTICLE_COUNT, CREAM);
                if action == CombatAction::HeavyStrike {
                    shake.trigger(shake_amplitude(dmg), camera_rest);
                }
            }
            CombatEvent::Crit { dmg } => {
                spawn_damage_text(
                    &mut commands,
                    at,
                    format!("{dmg} {CRIT_SUFFIX}"),
                    CRIT_FONT_SIZE,
                    CRIT_GOLD,
                    &ui_font,
                );
                spawn_particles(&mut commands, at, CRIT_PARTICLE_COUNT, CRIT_GOLD);
                shake.trigger(shake_amplitude(dmg), camera_rest);
            }
            CombatEvent::Blocked { dmg } => {
                spawn_damage_text(
                    &mut commands,
                    at,
                    format!("{BLOCKED_PREFIX} {dmg}"),
                    DAMAGE_FONT_SIZE,
                    BLOCKED_GRAY,
                    &ui_font,
                );
                spawn_particles(&mut commands, at, BLOCK_PARTICLE_COUNT, BLOCKED_GRAY);
            }
            CombatEvent::Missed | CombatEvent::OutOfReach => {
                spawn_damage_text(
                    &mut commands,
                    at,
                    MISS_TEXT.to_string(),
                    DAMAGE_FONT_SIZE,
                    CREAM,
                    &ui_font,
                );
            }
            _ => {}
        }
    }
}

// --- Plugin ---------------------------------------------------------------------

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenShake>()
            // Idempotent with SettingsPlugin's registration (the same
            // pattern AnimationPlugin already uses for CombatLogEvent):
            // keeps the arena's motion systems usable in apps/tests built
            // without SettingsPlugin, defaulting to full motion.
            .init_resource::<AccessibilityPreferences>()
            .add_message::<CombatLogEvent>()
            .add_systems(Startup, load_backgrounds)
            .add_systems(
                Update,
                (
                    spawn_combat_fx,
                    drift_parallax_layers,
                    animate_damage_text,
                    animate_particles,
                    apply_screen_shake,
                )
                    .run_if(in_state(GameState::Fight)),
            )
            .add_systems(OnExit(GameState::Fight), reset_screen_shake);
    }
}

#[cfg(test)]
mod tests {
    use super::super::ArenaPlugin;
    use super::*;
    use crate::character::Attributes;
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use bevy::state::app::StatesPlugin;
    use std::time::Duration;

    /// Headless app inside the fight arena at `progress` on the ladder.
    fn test_app_at(progress: LadderProgress) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes {
                putere: 4,
                agilitate: 2,
                vitalitate: 4,
                noroc: 3,
            },
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(progress);
        // Let `advance` jump the clock past the default 250 ms delta cap.
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(10));
        // Zero-length frames by default: MinimalPlugins' `Time` otherwise
        // advances by real wall-clock elapsed between `update()` calls,
        // which makes fade-alpha assertions flaky under load (more systems
        // in a frame's schedule = more wall time = a small but nonzero
        // fraction instead of the expected "just spawned" 1.0). `advance`
        // overrides this for the frames that need a real tick.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app
    }

    fn test_app() -> App {
        test_app_at(LadderProgress::default())
    }

    /// Sends one combat log event and runs a frame so FX spawn.
    fn send_event(app: &mut App, actor: CombatSide, action: CombatAction, event: CombatEvent) {
        app.world_mut().write_message(CombatLogEvent {
            actor,
            action,
            event,
        });
        app.update();
    }

    /// Advances the clock by exactly `seconds` and runs a frame.
    fn advance(app: &mut App, seconds: f32) {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(seconds),
        ));
        app.update();
        // Back to zero-length frames so later updates don't re-advance.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
    }

    fn damage_texts(app: &mut App) -> Vec<(String, f32, Color)> {
        app.world_mut()
            .query_filtered::<(&Text2d, &TextFont, &TextColor), With<DamageText>>()
            .iter(app.world())
            .map(|(text, font, color)| {
                let FontSize::Px(px) = font.font_size else {
                    panic!("damage numbers use pixel font sizes");
                };
                (text.0.clone(), px, color.0)
            })
            .collect()
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<C>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn ladder_progress_selects_the_tier_and_laps_wrap() {
        for (index, tier) in [
            (0, BackgroundTier::Village),
            (3, BackgroundTier::Village),
            (4, BackgroundTier::Forest),
            (6, BackgroundTier::Forest),
            (7, BackgroundTier::Mountains),
            (9, BackgroundTier::Mountains),
            (10, BackgroundTier::Village), // lap 2 starts over
            (17, BackgroundTier::Mountains),
        ] {
            assert_eq!(
                background_tier(LadderProgress(index)),
                tier,
                "ladder index {index}"
            );
        }
    }

    #[test]
    fn every_tier_maps_to_a_foreground_asset() {
        for (tier, path) in [
            (
                BackgroundTier::Village,
                "backgrounds/village_foreground.png",
            ),
            (BackgroundTier::Forest, "backgrounds/forest_foreground.png"),
            (
                BackgroundTier::Mountains,
                "backgrounds/mountains_foreground.png",
            ),
        ] {
            assert_eq!(tier.foreground_path(), path);
            assert!(
                path.ends_with("_foreground.png"),
                "{tier:?} foreground is distinct from parallax layers"
            );
        }
    }

    #[test]
    fn every_foreground_asset_exists_on_disk() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for tier in [
            BackgroundTier::Village,
            BackgroundTier::Forest,
            BackgroundTier::Mountains,
        ] {
            let path = manifest.join("assets").join(tier.foreground_path());
            assert!(
                path.is_file(),
                "{tier:?} foreground missing at {}",
                path.display()
            );
        }
    }

    #[test]
    fn each_tier_spawns_one_foreground_layer_tagged_for_cleanup() {
        for (progress, tier) in [
            (LadderProgress(0), BackgroundTier::Village),
            (LadderProgress(4), BackgroundTier::Forest),
            (LadderProgress(7), BackgroundTier::Mountains),
        ] {
            let mut app = test_app_at(progress);
            let layers: Vec<(BackgroundTier, f32, bool)> = app
                .world_mut()
                .query::<(&ArenaForeground, &Transform, Option<&ArenaScreen>)>()
                .iter(app.world())
                .map(|(foreground, transform, screen)| {
                    (foreground.tier, transform.translation.z, screen.is_some())
                })
                .collect();
            assert_eq!(layers, vec![(tier, -8.5, true)], "{tier:?}");
        }
    }

    #[test]
    fn the_arena_spawns_two_parallax_layers_that_drift_at_different_rates() {
        let mut app = test_app();
        let rates: Vec<f32> = app
            .world_mut()
            .query::<&ParallaxLayer>()
            .iter(app.world())
            .map(|layer| layer.rate)
            .collect();
        assert_eq!(rates.len(), 2, "far + near layer");
        assert_ne!(rates[0], rates[1], "layers move at different rates");

        advance(&mut app, 3.0);
        let offsets: Vec<f32> = app
            .world_mut()
            .query::<(&ParallaxLayer, &Transform)>()
            .iter(app.world())
            .map(|(layer, transform)| (transform.translation.x - layer.base_x).abs())
            .collect();
        assert!(
            offsets.iter().all(|offset| *offset > 0.0),
            "both layers drift: {offsets:?}"
        );
        let (min, max) = (
            offsets.iter().cloned().fold(f32::MAX, f32::min),
            offsets.iter().cloned().fold(0.0, f32::max),
        );
        assert!(min < max, "far layer drifts less than near: {offsets:?}");
    }

    #[test]
    fn reduced_motion_holds_parallax_layers_at_their_exact_rest_offset() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        advance(&mut app, 5.0);
        let offsets: Vec<f32> = app
            .world_mut()
            .query::<(&ParallaxLayer, &Transform)>()
            .iter(app.world())
            .map(|(layer, transform)| (transform.translation.x - layer.base_x).abs())
            .collect();
        assert!(
            offsets.iter().all(|offset| *offset == 0.0),
            "reduced motion holds every layer exactly at rest: {offsets:?}"
        );
    }

    #[test]
    fn toggling_reduced_motion_off_mid_scene_resumes_parallax_drift() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        advance(&mut app, 5.0);
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: false,
        });
        advance(&mut app, 3.0);
        let offsets: Vec<f32> = app
            .world_mut()
            .query::<(&ParallaxLayer, &Transform)>()
            .iter(app.world())
            .map(|(layer, transform)| (transform.translation.x - layer.base_x).abs())
            .collect();
        assert!(
            offsets.iter().any(|offset| *offset > 0.0),
            "drift resumes once reduced motion is turned back off: {offsets:?}"
        );
    }

    #[test]
    fn a_hit_shows_its_damage_number_and_a_small_spark_burst() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Hit { dmg: 6 },
        );
        assert_eq!(
            damage_texts(&mut app),
            vec![("6".to_string(), DAMAGE_FONT_SIZE, CREAM)]
        );
        assert_eq!(count::<HitParticle>(&mut app), HIT_PARTICLE_COUNT);
        assert!(
            !app.world().resource::<ScreenShake>().is_active(),
            "a plain strike hit does not shake the camera"
        );
    }

    #[test]
    fn a_crit_is_bigger_gold_and_bursts_more_sparks() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Enemy,
            CombatAction::QuickStrike,
            CombatEvent::Crit { dmg: 12 },
        );
        assert_eq!(
            damage_texts(&mut app),
            vec![(format!("12 {CRIT_SUFFIX}"), CRIT_FONT_SIZE, CRIT_GOLD)]
        );
        assert_eq!(count::<HitParticle>(&mut app), CRIT_PARTICLE_COUNT);
        assert!(app.world().resource::<ScreenShake>().is_active());
    }

    /// #214: hit, crit, block, and miss each carry a distinct text/shape
    /// cue — not just a different color — so they stay distinguishable in
    /// grayscale. This pins the whole set together so a future change to
    /// one outcome can't accidentally make it collide with another.
    #[test]
    fn hit_crit_block_and_miss_have_distinct_text_and_particle_counts() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Hit { dmg: 6 },
        );
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Crit { dmg: 12 },
        );
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Blocked { dmg: 3 },
        );
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Missed,
        );
        let texts: Vec<String> = damage_texts(&mut app)
            .into_iter()
            .map(|(text, _, _)| text)
            .collect();
        assert_eq!(
            texts,
            vec![
                "6".to_string(),
                format!("12 {CRIT_SUFFIX}"),
                format!("{BLOCKED_PREFIX} 3"),
                MISS_TEXT.to_string(),
            ],
            "each outcome's text is unique and reuses established log vocabulary"
        );
        // Every text is pairwise distinct (belt-and-suspenders on top of the
        // literal comparison above).
        for (i, a) in texts.iter().enumerate() {
            for b in &texts[i + 1..] {
                assert_ne!(a, b, "{texts:?}");
            }
        }
    }

    #[test]
    fn a_miss_floats_ratat_and_a_block_shows_the_chip_damage_with_a_few_particles() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Missed,
        );
        assert_eq!(count::<HitParticle>(&mut app), 0, "no sparks on miss");
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Blocked { dmg: 3 },
        );
        let texts: Vec<String> = damage_texts(&mut app)
            .into_iter()
            .map(|(text, _, _)| text)
            .collect();
        assert!(texts.contains(&MISS_TEXT.to_string()), "{texts:?}");
        assert!(texts.contains(&format!("{BLOCKED_PREFIX} 3")), "{texts:?}");
        assert_eq!(
            count::<HitParticle>(&mut app),
            BLOCK_PARTICLE_COUNT,
            "a block bursts a few (but visibly fewer-than-hit) chips — its own shape cue"
        );
    }

    #[test]
    fn damage_numbers_rise_fade_and_despawn_after_their_lifetime() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Hit { dmg: 6 },
        );
        let start_y = app
            .world_mut()
            .query_filtered::<&Transform, With<DamageText>>()
            .single(app.world())
            .expect("one damage number")
            .translation
            .y;
        advance(&mut app, 0.4);
        let (mid_y, alpha) = {
            let (transform, color) = app
                .world_mut()
                .query_filtered::<(&Transform, &TextColor), With<DamageText>>()
                .single(app.world())
                .expect("still alive at 0.4s");
            (transform.translation.y, color.0.alpha())
        };
        assert!(mid_y > start_y, "the number rises");
        assert!(alpha < 1.0, "the number fades");
        advance(&mut app, 0.5); // past the 0.8s lifetime
        assert_eq!(count::<DamageText>(&mut app), 0, "self-despawns");
    }

    #[test]
    fn particles_despawn_after_their_lifetime() {
        let mut app = test_app();
        send_event(
            &mut app,
            CombatSide::Enemy,
            CombatAction::QuickStrike,
            CombatEvent::Crit { dmg: 10 },
        );
        assert_eq!(count::<HitParticle>(&mut app), CRIT_PARTICLE_COUNT);
        advance(&mut app, PARTICLE_LIFETIME + 0.1);
        assert_eq!(count::<HitParticle>(&mut app), 0);
    }

    /// The camera translation.
    fn camera_at(app: &mut App) -> Vec3 {
        app.world_mut()
            .query_filtered::<&Transform, With<crate::core::WorldCamera>>()
            .single(app.world())
            .expect("one camera")
            .translation
    }

    #[test]
    fn the_camera_shakes_on_a_heavy_strike_hit_and_returns_exactly_to_rest() {
        let mut app = test_app();
        let rest = camera_at(&mut app);
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::HeavyStrike,
            CombatEvent::Hit { dmg: 12 },
        );
        advance(&mut app, 0.1);
        assert_ne!(camera_at(&mut app), rest, "mid-shake the camera is offset");
        advance(&mut app, SHAKE_DURATION);
        assert_eq!(
            camera_at(&mut app),
            rest,
            "after the shake the camera is exactly at rest"
        );
        assert!(!app.world().resource::<ScreenShake>().is_active());
    }

    #[test]
    fn reduced_motion_disables_camera_displacement_but_the_shake_still_times_out_identically() {
        let mut app = test_app();
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        let rest = camera_at(&mut app);
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::HeavyStrike,
            CombatEvent::Hit { dmg: 12 },
        );
        advance(&mut app, 0.1);
        assert_eq!(
            camera_at(&mut app),
            rest,
            "reduced motion never displaces the camera"
        );
        assert!(
            app.world().resource::<ScreenShake>().is_active(),
            "the shake's own bookkeeping (and thus its timing) is unaffected"
        );
        advance(&mut app, SHAKE_DURATION);
        assert_eq!(camera_at(&mut app), rest);
        assert!(
            !app.world().resource::<ScreenShake>().is_active(),
            "the shake deactivates after exactly SHAKE_DURATION, same as full motion"
        );
    }

    #[test]
    fn toggling_reduced_motion_on_mid_shake_snaps_the_camera_back_to_rest() {
        let mut app = test_app();
        let rest = camera_at(&mut app);
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::HeavyStrike,
            CombatEvent::Hit { dmg: 12 },
        );
        advance(&mut app, 0.1);
        assert_ne!(camera_at(&mut app), rest, "mid-shake the camera is offset");
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        });
        advance(&mut app, 0.01);
        assert_eq!(
            camera_at(&mut app),
            rest,
            "flipping the preference mid-shake restores the camera immediately, nothing stuck off-center"
        );
    }

    #[test]
    fn leaving_the_fight_mid_fx_despawns_everything_and_rests_the_camera() {
        let mut app = test_app();
        let rest = camera_at(&mut app);
        send_event(
            &mut app,
            CombatSide::Enemy,
            CombatAction::HeavyStrike,
            CombatEvent::Crit { dmg: 20 },
        );
        send_event(
            &mut app,
            CombatSide::Player,
            CombatAction::QuickStrike,
            CombatEvent::Missed,
        );
        advance(&mut app, 0.05); // FX alive, camera shaking
        assert!(count::<DamageText>(&mut app) > 0);
        assert!(count::<HitParticle>(&mut app) > 0);

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();

        assert_eq!(count::<DamageText>(&mut app), 0, "no numbers leak");
        assert_eq!(count::<HitParticle>(&mut app), 0, "no sparks leak");
        assert_eq!(count::<ParallaxLayer>(&mut app), 0, "no layers leak");
        assert_eq!(count::<ArenaForeground>(&mut app), 0, "no foreground leaks");
        assert_eq!(camera_at(&mut app), rest, "camera restored on exit");
        assert!(!app.world().resource::<ScreenShake>().is_active());
    }
}
