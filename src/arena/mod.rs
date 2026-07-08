//! Arena scene for `GameState::Fight`: placeholder scenery quads, the
//! player's fighter (from [`PlayerCharacter`]) on the left, and the current
//! [`LadderProgress`] opponent from the folklore roster on the right —
//! attributes (lap-scaled), AI profile, and equipment all come from the
//! ladder data. Fighters render as animated sprite-sheet characters (#22,
//! see [`animation`]); they only spawn once their sheets are loaded, which
//! also gates the start of combat (the combat turn waits for the fighters).

pub mod animation;
pub mod fx;

use bevy::prelude::*;

use crate::character::{Attributes, EnemyFighter, PlayerFighter, spawn_fighter};
use crate::combat::AiProfile;
use crate::core::{GameState, UiFont, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::cutout::{
    CutoutRig, CutoutRigTemplate, CutoutTemplate, boss_template, enemy_template, human_template,
    human_template_for, spawn_cutout_rig,
};
use crate::items::{Equipment, GearMotion, ItemId, Slot, item_visual};
use crate::roster::{Boss, LadderProgress, Opponent};
use crate::theme::{BOSS_LABEL_COLOR, CREAM, GROUND_COLOR};
use animation::{AnimationSet, FighterClip, FighterSpriteSheets};
use fx::{ArenaBackgrounds, background_tier, spawn_background};

/// Logical resolution the scene is designed for (the window in `main.rs`).
pub const ARENA_WIDTH: f32 = 800.0;
/// Logical height matching [`ARENA_WIDTH`].
pub const ARENA_HEIGHT: f32 = 600.0;

/// Rendered size of a fighter: one 128x128 sprite-sheet frame.
pub const FIGHTER_SIZE: Vec2 = Vec2::new(128.0, 128.0);

/// Height of the ground strip along the bottom of the arena.
const GROUND_HEIGHT: f32 = 120.0;
/// World-space y of the top edge of the ground; fighters stand on it.
const GROUND_TOP_Y: f32 = -ARENA_HEIGHT / 2.0 + GROUND_HEIGHT;
/// World-space y of a fighter standing on the ground.
const FIGHTER_Y: f32 = GROUND_TOP_Y + FIGHTER_SIZE.y / 2.0;

/// Where the player's fighter stands, facing right. The Phase 4 animation
/// issue reuses this anchor.
pub const PLAYER_ANCHOR: Transform = Transform::from_xyz(-220.0, FIGHTER_Y, 0.0);
/// Where the opponent stands, facing left; mirrors [`PLAYER_ANCHOR`].
pub const ENEMY_ANCHOR: Transform = Transform::from_xyz(220.0, FIGHTER_Y, 0.0);

/// Vertical offset of the name label above a fighter's body center.
const LABEL_OFFSET_Y: f32 = FIGHTER_SIZE.y / 2.0 + 24.0;

/// Marker for every arena entity; all of them despawn on
/// `OnExit(GameState::Fight)` via [`despawn_screen`].
#[derive(Component)]
pub(crate) struct ArenaScreen;

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((animation::AnimationPlugin, fx::FxPlugin))
            .add_systems(OnEnter(GameState::Fight), spawn_arena)
            .add_systems(
                Update,
                (
                    spawn_arena_when_ready,
                    spawn_equipped_gear_layers,
                    sync_gear_visual_layers,
                )
                    .chain()
                    .after(AnimationSet::Apply)
                    .run_if(in_state(GameState::Fight)),
            )
            .add_systems(OnExit(GameState::Fight), despawn_screen::<ArenaScreen>);
    }
}

/// `OnEnter(Fight)` entry point: spawns the scene immediately when the
/// sprite sheets are already loaded (the usual case — loading starts at app
/// startup). Warns and skips if no [`PlayerCharacter`] was confirmed, which
/// only happens if the state flow is driven out of order.
fn spawn_arena(
    commands: Commands,
    player: Option<Res<PlayerCharacter>>,
    ladder: Option<Res<LadderProgress>>,
    sheets: Res<FighterSpriteSheets>,
    backgrounds: Res<ArenaBackgrounds>,
    asset_server: Option<Res<AssetServer>>,
    ui_font: Res<UiFont>,
) {
    let Some(player) = player else {
        warn!("entered GameState::Fight without a PlayerCharacter; arena not spawned");
        return;
    };
    if !sheets.ready(asset_server.as_deref()) {
        // The loading guard: `spawn_arena_when_ready` retries every frame.
        // Combat cannot start either — its turn resource waits for the
        // fighters to exist.
        return;
    }
    spawn_scene(
        commands,
        &player,
        ladder,
        &backgrounds,
        &ui_font,
        asset_server.as_deref(),
    );
}

/// The loading-guard retry: once the sheets finish loading mid-fight-screen,
/// spawns the scene that [`spawn_arena`] skipped.
// A Bevy system: each parameter is a distinct ECS handle the scene spawn needs.
#[allow(clippy::too_many_arguments)]
fn spawn_arena_when_ready(
    commands: Commands,
    player: Option<Res<PlayerCharacter>>,
    ladder: Option<Res<LadderProgress>>,
    sheets: Res<FighterSpriteSheets>,
    backgrounds: Res<ArenaBackgrounds>,
    asset_server: Option<Res<AssetServer>>,
    spawned: Query<(), With<ArenaScreen>>,
    ui_font: Res<UiFont>,
) {
    let Some(player) = player else {
        return; // spawn_arena already warned
    };
    if !spawned.is_empty() || !sheets.ready(asset_server.as_deref()) {
        return;
    }
    spawn_scene(
        commands,
        &player,
        ladder,
        &backgrounds,
        &ui_font,
        asset_server.as_deref(),
    );
}

/// Builds the whole fight scene: scenery quads, the player's fighter, and
/// the current ladder opponent (attributes lap-scaled, AI profile and
/// equipment from the roster data; bosses get the [`Boss`] tag and the
/// distinct label color).
fn spawn_scene(
    mut commands: Commands,
    player: &PlayerCharacter,
    ladder: Option<Res<LadderProgress>>,
    backgrounds: &ArenaBackgrounds,
    ui_font: &UiFont,
    asset_server: Option<&AssetServer>,
) {
    let ladder = ladder.map(|ladder| *ladder).unwrap_or_default();
    let opponent = ladder.opponent();
    spawn_background(&mut commands, backgrounds, background_tier(ladder));
    spawn_scenery(&mut commands);
    let player_fighter = spawn_arena_fighter(
        &mut commands,
        player.name.clone(),
        player.attributes,
        PlayerFighter,
        PLAYER_ANCHOR,
        human_template_for(player.appearance),
        false,
        CREAM,
        ui_font,
        asset_server,
    );
    commands.entity(player_fighter).insert(player.appearance);
    let enemy = spawn_arena_fighter(
        &mut commands,
        ladder.display_name(),
        ladder.attributes(),
        (
            EnemyFighter,
            AiProfile {
                aggression: opponent.aggression,
            },
        ),
        ENEMY_ANCHOR,
        opponent_template(opponent),
        true,
        if opponent.is_boss {
            BOSS_LABEL_COLOR
        } else {
            CREAM
        },
        ui_font,
        asset_server,
    );
    let mut equipment = Equipment::default();
    for &id in opponent.equipment {
        equipment.equip(id);
    }
    commands.entity(enemy).insert(equipment);
    if opponent.is_boss {
        commands.entity(enemy).insert(Boss {
            intro_line: opponent.intro_line,
        });
    }
}

/// The ground strip the fighters stand on, in front of the parallax
/// background layers (see [`fx::spawn_background`]).
fn spawn_scenery(commands: &mut Commands) {
    commands.spawn((
        ArenaScreen,
        Sprite::from_color(GROUND_COLOR, Vec2::new(ARENA_WIDTH, GROUND_HEIGHT)),
        Transform::from_xyz(0.0, (-ARENA_HEIGHT + GROUND_HEIGHT) / 2.0, -9.0),
    ));
}

/// Spawns one fighter through the shared [`spawn_fighter`] (so it carries the
/// #8 components and full pools), then dresses it with the arena visuals: its
/// animated sprite at its anchor (starting on the idle loop) and a
/// world-space name label above (in `label_color`, so bosses read
/// differently at a glance).
// Each argument is one distinct piece of the fighter's dressing; bundling
// them into a struct for one call site would only add indirection.
#[allow(clippy::too_many_arguments)]
fn spawn_arena_fighter(
    commands: &mut Commands,
    name: impl Into<String>,
    attrs: Attributes,
    marker: impl Bundle,
    anchor: Transform,
    template: CutoutRigTemplate,
    flip_x: bool,
    label_color: Color,
    ui_font: &UiFont,
    asset_server: Option<&AssetServer>,
) -> Entity {
    let name = name.into();
    let label = name.clone();
    let fighter = spawn_fighter(commands, name, attrs, marker);
    commands
        .entity(fighter)
        .insert((
            ArenaScreen,
            FighterClip::Idle,
            FighterClip::Idle.animation(),
            anchor,
        ))
        .with_children(|body| {
            body.spawn((
                Text2d::new(label),
                ui_font.text_font(20.0),
                TextColor(label_color),
                Transform::from_xyz(0.0, LABEL_OFFSET_Y, 0.1),
            ));
        });
    spawn_cutout_rig(commands, fighter, template, asset_server, flip_x);
    fighter
}

fn opponent_template(opponent: &Opponent) -> CutoutRigTemplate {
    match opponent.cutout_template {
        CutoutTemplate::Human => human_template(),
        CutoutTemplate::Enemy => enemy_template(),
        CutoutTemplate::Boss => boss_template(),
    }
}

/// One visible equipment overlay attached to a fighter.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct GearVisualLayer {
    /// Catalog item shown by this layer.
    pub item: ItemId,
    /// Equipment slot this layer occupies.
    pub slot: Slot,
    /// Motion profile used to follow the owning fighter's pose.
    pub motion: GearMotion,
    /// Stable draw order relative to the fighter body.
    pub z_offset: f32,
}

/// Last clip/frame copied from the owning fighter into a gear visual layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GearAnimationState {
    pub clip: FighterClip,
    pub frame: usize,
}

/// Query alias for fighters whose equipment changed this frame.
type ChangedFighterEquipment<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Equipment),
    (With<crate::character::Fighter>, Changed<Equipment>),
>;

/// Spawns gear overlays from each fighter's current equipment.
/// The shop copies [`crate::shop::PlayerEquipment`] onto the player fighter
/// after spawn, and roster opponents also carry [`Equipment`]. This system
/// reacts to both.
fn spawn_equipped_gear_layers(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    fighters: ChangedFighterEquipment,
    existing_layers: Query<(Entity, &ChildOf), With<GearVisualLayer>>,
) {
    for (fighter, equipment) in &fighters {
        for (layer, child_of) in &existing_layers {
            if child_of.parent() == fighter {
                commands.entity(layer).despawn();
            }
        }

        for slot in Slot::ALL {
            let Some(item) = equipment.equipped(slot) else {
                continue;
            };
            let Some(visual) = item_visual(item) else {
                continue;
            };
            commands.entity(fighter).with_children(|body| {
                body.spawn((
                    GearVisualLayer {
                        item,
                        slot,
                        motion: visual.motion,
                        z_offset: visual.z_offset,
                    },
                    GearAnimationState {
                        clip: FighterClip::Idle,
                        frame: FighterClip::Idle.animation().first,
                    },
                    gear_sprite(visual.fallback_asset_path(), asset_server.as_deref()),
                    gear_local_transform(
                        visual.motion,
                        FighterClip::Idle,
                        FighterClip::Idle.animation().first,
                        visual.z_offset,
                        false,
                    ),
                ));
            });
        }
    }
}

/// Runtime uses the generated transparent PNG; headless tests without an
/// [`AssetServer`] still spawn a harmless placeholder sprite so ECS behavior
/// remains testable.
fn gear_sprite(asset_path: &'static str, asset_server: Option<&AssetServer>) -> Sprite {
    if let Some(asset_server) = asset_server {
        Sprite::from_image(asset_server.load(asset_path))
    } else {
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.35), FIGHTER_SIZE)
    }
}

/// Copies the owning fighter's current animation state into every gear layer
/// and applies a small local transform so static overlay art feels attached
/// to hands, shield arm, head, torso, or feet during each clip.
fn sync_gear_visual_layers(
    fighters: Query<(
        &FighterClip,
        &animation::SpriteAnimation,
        Option<&Sprite>,
        Option<&CutoutRig>,
    )>,
    mut layers: Query<(
        &GearVisualLayer,
        &ChildOf,
        &mut GearAnimationState,
        &mut Transform,
    )>,
) {
    for (layer, child_of, mut state, mut transform) in &mut layers {
        let Ok((clip, anim, sprite, rig)) = fighters.get(child_of.parent()) else {
            continue;
        };
        let frame = sprite
            .and_then(|sprite| sprite.texture_atlas.as_ref())
            .as_ref()
            .map(|atlas| atlas.index)
            .unwrap_or_else(|| anim.current_frame());
        let flip_x = sprite
            .map(|sprite| sprite.flip_x)
            .unwrap_or_else(|| rig.map(|rig| rig.flip_x).unwrap_or(false));
        *state = GearAnimationState { clip: *clip, frame };
        *transform = gear_local_transform(layer.motion, *clip, frame, layer.z_offset, flip_x);
    }
}

/// Presentation offsets for static gear overlays. The image stays the same,
/// but the attachment transform changes with clip/frame so weapons and
/// shields read as part of the animated fighter rather than a pasted layer.
fn gear_local_transform(
    motion: GearMotion,
    clip: FighterClip,
    frame: usize,
    z_offset: f32,
    flip_x: bool,
) -> Transform {
    let first = clip.animation().first;
    let frame_in_clip = frame.saturating_sub(first) as f32;
    let (mut x, y, rotation) = match motion {
        GearMotion::WeaponHand => weapon_pose(clip, frame_in_clip),
        GearMotion::ShieldArm => shield_pose(clip, frame_in_clip),
        GearMotion::Body => body_pose(clip, frame_in_clip),
        GearMotion::Head => head_pose(clip, frame_in_clip),
        GearMotion::Feet => feet_pose(clip, frame_in_clip),
    };
    let mut angle = rotation;
    if flip_x {
        x = -x;
        angle = -angle;
    }
    Transform {
        translation: Vec3::new(x, y, z_offset),
        rotation: Quat::from_rotation_z(angle),
        scale: Vec3::ONE,
    }
}

fn weapon_pose(clip: FighterClip, frame: f32) -> (f32, f32, f32) {
    match clip {
        FighterClip::Attack => (4.0 + frame * 5.0, 1.0 - frame * 1.5, -0.12 - frame * 0.08),
        FighterClip::Hurt => (-3.0, -1.0, 0.12),
        FighterClip::Ko => (-7.0, -7.0, 0.42),
        FighterClip::StepForward => (3.0 + frame * 2.0, -1.0, -0.08),
        FighterClip::StepBack => (-3.0 - frame * 2.0, 1.0, 0.08),
        FighterClip::Idle => (0.0, idle_bob(frame), 0.0),
    }
}

fn shield_pose(clip: FighterClip, frame: f32) -> (f32, f32, f32) {
    match clip {
        FighterClip::Attack => (-2.0 + frame, 1.0, 0.04),
        FighterClip::Hurt => (-5.0, 0.0, -0.12),
        FighterClip::Ko => (-8.0, -5.0, -0.22),
        FighterClip::StepForward => (1.0 + frame, 0.0, 0.03),
        FighterClip::StepBack => (-2.0 - frame, 1.0, -0.03),
        FighterClip::Idle => (0.0, idle_bob(frame), 0.0),
    }
}

fn body_pose(clip: FighterClip, frame: f32) -> (f32, f32, f32) {
    match clip {
        FighterClip::Hurt => (-2.0, -1.0, -0.04),
        FighterClip::Ko => (-5.0, -6.0, 0.12),
        FighterClip::StepForward => (frame, -1.0, 0.0),
        FighterClip::StepBack => (-frame, 1.0, 0.0),
        _ => (0.0, idle_bob(frame), 0.0),
    }
}

fn head_pose(clip: FighterClip, frame: f32) -> (f32, f32, f32) {
    match clip {
        FighterClip::Attack => (2.0 + frame, -1.0, -0.04),
        FighterClip::Hurt => (-3.0, -2.0, 0.08),
        FighterClip::Ko => (-6.0, -8.0, 0.18),
        FighterClip::StepForward => (frame, 0.0, 0.0),
        FighterClip::StepBack => (-frame, 1.0, 0.0),
        FighterClip::Idle => (0.0, idle_bob(frame), 0.0),
    }
}

fn feet_pose(clip: FighterClip, frame: f32) -> (f32, f32, f32) {
    match clip {
        FighterClip::Attack => (2.0, -1.0, 0.0),
        FighterClip::Hurt => (-2.0, -1.0, -0.03),
        FighterClip::Ko => (-4.0, -4.0, 0.08),
        FighterClip::StepForward => (3.0 + frame * 2.0, -1.0, 0.0),
        FighterClip::StepBack => (-3.0 - frame * 2.0, 1.0, 0.0),
        FighterClip::Idle => (0.0, idle_bob(frame) * 0.5, 0.0),
    }
}

fn idle_bob(frame: f32) -> f32 {
    if frame.rem_euclid(2.0) < 1.0 {
        0.0
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{
        AccentColor, BodyBuild, Fighter, FighterName, HairStyle, Health, PlayerAppearance,
        SkinTone, Stamina, stats,
    };
    use crate::core::CorePlugin;
    use crate::cutout::{
        CutoutPartMarker, CutoutRig, CutoutTemplate, boss_template, enemy_template, human_template,
    };
    use crate::items::{ItemId, Slot};
    use crate::roster::LADDER;
    use bevy::state::app::StatesPlugin;

    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    fn player_character() -> PlayerCharacter {
        PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
        }
    }

    /// Headless app already inside the fight arena, at the start of the run
    /// (first ladder opponent: the Hoț de codru).
    fn test_app() -> App {
        test_app_at(LadderProgress::default())
    }

    /// Headless app already inside the fight arena, at `progress` on the
    /// ladder.
    fn test_app_at(progress: LadderProgress) -> App {
        test_app_with(player_character(), progress)
    }

    fn test_app_with(player: PlayerCharacter, progress: LadderProgress) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(player);
        app.insert_resource(progress);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter spawn
        app
    }

    fn leave_fight(app: &mut App) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update(); // transition + OnExit cleanup
    }

    #[test]
    fn entering_fight_spawns_one_player_and_one_enemy_with_full_pools() {
        let mut app = test_app();

        let players: Vec<(Health, Stamina)> = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<PlayerFighter>>()
            .iter(app.world())
            .map(|(h, s)| (*h, *s))
            .collect();
        let player_hp = stats::max_hp(&PLAYER_ATTRIBUTES);
        let player_stamina = stats::max_stamina(&PLAYER_ATTRIBUTES);
        assert_eq!(
            players,
            vec![(
                Health {
                    current: player_hp,
                    max: player_hp,
                },
                Stamina {
                    current: player_stamina,
                    max: player_stamina,
                },
            )],
            "exactly one player fighter at full pools per the #8 formulas"
        );

        let enemies: Vec<(Health, Stamina)> = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<EnemyFighter>>()
            .iter(app.world())
            .map(|(h, s)| (*h, *s))
            .collect();
        assert_eq!(
            enemies,
            vec![(
                Health {
                    current: 70,
                    max: 70,
                },
                Stamina {
                    current: 40,
                    max: 40,
                },
            )],
            "exactly one Hoț de codru (2/2/2/1) at full pools"
        );
    }

    #[test]
    fn player_carries_the_exact_creation_name_and_attributes() {
        let mut app = test_app();
        let (name, attrs) = app
            .world_mut()
            .query_filtered::<(&FighterName, &Attributes), With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        assert_eq!(name.0, "Făt-Frumos");
        assert_eq!(*attrs, PLAYER_ATTRIBUTES);
    }

    #[test]
    fn the_enemy_carries_its_roster_ai_profile_and_the_player_none() {
        let mut app = test_app();
        let profile = app
            .world_mut()
            .query_filtered::<&AiProfile, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter has an AI profile");
        assert_eq!(
            *profile,
            AiProfile {
                aggression: LADDER[0].aggression,
            },
            "the Hoț de codru fights with its low roster aggression"
        );
        assert!(
            app.world_mut()
                .query_filtered::<&AiProfile, With<PlayerFighter>>()
                .single(app.world())
                .is_err(),
            "the player is human-driven and carries no AI profile"
        );
    }

    #[test]
    fn fighters_stand_at_the_anchor_transforms_on_opposite_sides() {
        let mut app = test_app();
        let player = app
            .world_mut()
            .query_filtered::<&Transform, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists")
            .translation;
        let enemy = app
            .world_mut()
            .query_filtered::<&Transform, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists")
            .translation;
        assert_eq!(player, PLAYER_ANCHOR.translation);
        assert_eq!(enemy, ENEMY_ANCHOR.translation);
        assert!(player.x < 0.0 && enemy.x > 0.0, "opposite sides");
    }

    fn cutout_child_count(app: &mut App, root: Entity) -> usize {
        let children = app
            .world()
            .get::<Children>(root)
            .expect("fighter has cutout children")
            .to_vec();
        children
            .into_iter()
            .filter(|child| app.world().get::<CutoutPartMarker>(*child).is_some())
            .count()
    }

    fn template_part_count(template: CutoutTemplate) -> usize {
        match template {
            CutoutTemplate::Human => human_template().parts.len(),
            CutoutTemplate::Enemy => enemy_template().parts.len(),
            CutoutTemplate::Boss => boss_template().parts.len(),
        }
    }

    #[test]
    fn arena_fighters_spawn_cutout_rigs_instead_of_root_body_sprites() {
        let mut app = test_app();
        let (player, player_rig, player_has_sprite) = app
            .world_mut()
            .query_filtered::<(Entity, &CutoutRig, Has<Sprite>), With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter has a cutout rig");
        assert_eq!(player_rig.template, CutoutTemplate::Human);
        assert!(!player_rig.flip_x);
        assert!(
            !player_has_sprite,
            "the body is rendered by cutout children, not a root sprite sheet"
        );
        assert_eq!(
            cutout_child_count(&mut app, player),
            human_template().parts.len()
        );

        let (enemy, enemy_rig, enemy_has_sprite) = app
            .world_mut()
            .query_filtered::<(Entity, &CutoutRig, Has<Sprite>), With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter has a cutout rig");
        let enemy_template = enemy_rig.template;
        assert_eq!(enemy_template, LADDER[0].cutout_template);
        assert!(
            enemy_rig.flip_x,
            "enemy source art is mirrored in the arena"
        );
        assert!(!enemy_has_sprite);
        let enemy_template = enemy_rig.template;
        assert_eq!(
            cutout_child_count(&mut app, enemy),
            template_part_count(enemy_template)
        );
    }

    #[test]
    fn player_fighter_carries_the_confirmed_appearance_component() {
        let appearance = PlayerAppearance {
            skin_tone: SkinTone::Deep,
            build: BodyBuild::Powerful,
            hair: HairStyle::Tied,
            accent: AccentColor::Storm,
        };
        let mut app = test_app_with(
            PlayerCharacter {
                name: "Făt-Frumos".to_string(),
                attributes: PLAYER_ATTRIBUTES,
                appearance,
            },
            LadderProgress::default(),
        );

        let stored = app
            .world_mut()
            .query_filtered::<&PlayerAppearance, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter carries appearance");
        assert_eq!(*stored, appearance);
    }

    #[test]
    fn both_fighters_have_name_labels_above_them() {
        let mut app = test_app();
        let mut labels: Vec<String> = app
            .world_mut()
            .query::<(&Text2d, &ChildOf, &Transform)>()
            .iter(app.world())
            .map(|(text, child_of, transform)| {
                assert!(
                    app.world().entity(child_of.parent()).contains::<Fighter>(),
                    "label is attached to a fighter"
                );
                assert!(transform.translation.y > 0.0, "label sits above the body");
                text.0.clone()
            })
            .collect();
        labels.sort();
        assert_eq!(
            labels,
            vec!["Făt-Frumos".to_string(), "Hoț de codru".to_string()]
        );
    }

    /// The enemy's `(FighterName, Attributes, Equipment, Option<Boss>)` and
    /// its label's `TextColor`.
    fn enemy_snapshot(app: &mut App) -> (String, Attributes, Equipment, Option<Boss>, Color) {
        let (entity, name, attrs, equipment, boss) = app
            .world_mut()
            .query_filtered::<(Entity, &FighterName, &Attributes, &Equipment, Option<&Boss>), With<EnemyFighter>>()
            .single(app.world())
            .expect("exactly one enemy fighter");
        let (name, attrs, equipment, boss) =
            (name.0.clone(), *attrs, equipment.clone(), boss.copied());
        let label_color = app
            .world_mut()
            .query::<(&TextColor, &ChildOf)>()
            .iter(app.world())
            .find(|(_, child_of)| child_of.parent() == entity)
            .map(|(color, _)| color.0)
            .expect("the enemy has a name label");
        (name, attrs, equipment, boss, label_color)
    }

    #[test]
    fn the_ladder_position_picks_the_spawned_opponent() {
        // LadderProgress(4) is the acceptance-criteria fight: Muma Pădurii,
        // the first boss.
        let mut app = test_app_at(LadderProgress(4));
        let (name, attrs, _, boss, label_color) = enemy_snapshot(&mut app);
        let rig = app
            .world_mut()
            .query_filtered::<&CutoutRig, With<EnemyFighter>>()
            .single(app.world())
            .expect("boss enemy has a cutout rig");
        assert_eq!(name, "Muma Pădurii");
        assert_eq!(attrs, LADDER[4].attrs);
        assert_eq!(
            boss,
            Some(Boss {
                intro_line: LADDER[4].intro_line,
            }),
            "the boss flag rides on the fighter"
        );
        assert_eq!(label_color, BOSS_LABEL_COLOR, "boss label color");
        assert_eq!(rig.template, CutoutTemplate::Boss, "boss template");
    }

    #[test]
    fn a_non_human_ladder_enemy_uses_the_enemy_template() {
        let mut app = test_app_at(LadderProgress(1));
        let (_, rig) = app
            .world_mut()
            .query_filtered::<(&FighterName, &CutoutRig), With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        assert_eq!(rig.template, CutoutTemplate::Enemy);
    }

    #[test]
    fn a_boss_ladder_enemy_uses_the_boss_template() {
        let mut app = test_app_at(LadderProgress(4));
        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy exists");
        let (_, rig) = app
            .world_mut()
            .query_filtered::<(&FighterName, &CutoutRig), With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        assert_eq!(rig.template, CutoutTemplate::Boss);
        assert_eq!(
            cutout_child_count(&mut app, enemy),
            boss_template().parts.len()
        );
    }

    #[test]
    fn a_large_non_boss_enemy_can_still_use_the_boss_template() {
        let mut app = test_app_at(LadderProgress(8));
        let (name, _, _, boss, _) = enemy_snapshot(&mut app);
        let (_, rig) = app
            .world_mut()
            .query_filtered::<(&FighterName, &CutoutRig), With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        assert_eq!(name, "Zmeu");
        assert_eq!(boss, None, "Zmeu is not a roster boss");
        assert_eq!(rig.template, CutoutTemplate::Boss);
    }

    #[test]
    fn a_regular_opponent_carries_no_boss_tag_and_the_cream_label() {
        let mut app = test_app();
        let (name, _, equipment, boss, label_color) = enemy_snapshot(&mut app);
        assert_eq!(name, "Hoț de codru");
        assert_eq!(boss, None);
        assert_eq!(label_color, CREAM);
        assert_eq!(equipment, Equipment::default(), "the Hoț fights bare");
    }

    #[test]
    fn equipped_opponents_spawn_with_their_roster_gear() {
        let mut app = test_app_at(LadderProgress(9));
        let (name, _, equipment, boss, _) = enemy_snapshot(&mut app);
        assert_eq!(name, "Zmeul Zmeilor");
        assert!(boss.is_some());
        assert_eq!(
            equipment.equipped(Slot::Weapon),
            Some(ItemId::BuzduganCuTreiPeceti)
        );
        assert_eq!(equipment.equipped(Slot::Torso), Some(ItemId::CamasaDeZale));
    }

    #[test]
    fn equipped_opponents_spawn_visual_layers_from_roster_gear() {
        let mut app = test_app_at(LadderProgress(9));
        app.update();

        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        let mut layers: Vec<(ItemId, Slot, GearMotion, Entity)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter(|(_, child_of)| child_of.parent() == enemy)
            .map(|(layer, child_of)| (layer.item, layer.slot, layer.motion, child_of.parent()))
            .collect();
        layers.sort_by_key(|(id, _, _, _)| *id as usize);
        assert_eq!(
            layers,
            vec![
                (
                    ItemId::BuzduganCuTreiPeceti,
                    Slot::Weapon,
                    GearMotion::WeaponHand,
                    enemy
                ),
                (ItemId::CamasaDeZale, Slot::Torso, GearMotion::Body, enemy),
            ],
            "roster equipment is visible, not only stat-bearing"
        );
    }

    #[test]
    fn a_second_lap_opponent_is_stronger_and_labeled_with_the_lap() {
        let mut app = test_app_at(LadderProgress(10));
        let (name, attrs, _, boss, _) = enemy_snapshot(&mut app);
        assert_eq!(name, "Hoț de codru (Turul 2)");
        assert_eq!(boss, None);
        use crate::roster::attribute_total;
        let total = attribute_total(&attrs);
        assert_eq!(total, 8, "total 7 scaled by 1.2 rounds to 8");
        assert!(
            total > attribute_total(&LADDER[0].attrs),
            "lap 2 is measurably stronger"
        );
    }

    #[test]
    fn scenery_quads_surround_the_fighters() {
        let mut app = test_app();
        let scenery = app
            .world_mut()
            .query_filtered::<(), (With<ArenaScreen>, With<Sprite>, Without<Fighter>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            scenery, 4,
            "two parallax layers + foreground depth + ground"
        );
    }

    #[test]
    fn leaving_fight_despawns_every_arena_entity() {
        let mut app = test_app();
        leave_fight(&mut app);

        let arena = app
            .world_mut()
            .query_filtered::<(), With<ArenaScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(arena, 0, "no tagged arena entities remain");
        let fighters = app
            .world_mut()
            .query_filtered::<(), With<Fighter>>()
            .iter(app.world())
            .count();
        assert_eq!(fighters, 0, "no fighters remain");
        let labels = app.world_mut().query::<&Text2d>().iter(app.world()).count();
        assert_eq!(labels, 0, "name labels despawn with their fighters");
    }

    #[test]
    fn entering_fight_without_a_player_character_spawns_nothing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        let arena = app
            .world_mut()
            .query_filtered::<(), With<ArenaScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(arena, 0);
    }

    #[test]
    fn equipped_player_items_spawn_visual_layers_from_the_shop_loadout() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::CojocGros);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            ArenaPlugin,
            crate::shop::ShopPlugin,
        ));
        app.insert_resource(player_character());
        app.insert_resource(crate::shop::PlayerEquipment(loadout));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app.update();

        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        let mut layers: Vec<(ItemId, Slot, Entity)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter(|(_, child_of)| child_of.parent() == player)
            .map(|(layer, child_of)| (layer.item, layer.slot, child_of.parent()))
            .collect();
        layers.sort_by_key(|(id, _, _)| *id as usize);
        assert_eq!(
            layers,
            vec![
                (ItemId::Palos, Slot::Weapon, player),
                (ItemId::CojocGros, Slot::Torso, player),
            ]
        );
    }

    fn player_with_loadout(loadout: Equipment) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            ArenaPlugin,
            crate::shop::ShopPlugin,
        ));
        app.insert_resource(player_character());
        app.insert_resource(crate::shop::PlayerEquipment(loadout));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app.update();
        app
    }

    fn set_player_clip(app: &mut App, clip: FighterClip, frame: usize) {
        let world = app.world_mut();
        let (mut current, mut animation) = world
            .query_filtered::<(&mut FighterClip, &mut animation::SpriteAnimation), With<PlayerFighter>>()
            .single_mut(world)
            .expect("player fighter exists");
        *current = clip;
        *animation = clip.animation();
        animation.set_current_frame(frame);
    }

    #[test]
    fn gear_layers_receive_clip_and_frame_state_from_the_owner() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        let mut app = player_with_loadout(loadout);

        let attack_frame = FighterClip::Attack.animation().first + 2;
        set_player_clip(&mut app, FighterClip::Attack, attack_frame);
        app.update();

        let (state, transform) = app
            .world_mut()
            .query_filtered::<(&GearAnimationState, &Transform), With<GearVisualLayer>>()
            .single(app.world())
            .expect("one gear layer exists");
        assert_eq!(state.clip, FighterClip::Attack);
        assert!(
            (FighterClip::Attack.animation().first..=FighterClip::Attack.animation().last)
                .contains(&state.frame),
            "gear frame stays in the attack clip"
        );
        assert!(
            transform.translation.x > 0.0 || transform.rotation != Quat::IDENTITY,
            "weapon layer moves away from its idle attachment"
        );
    }

    #[test]
    fn gear_layers_follow_back_to_idle_after_the_owner_clip_resets() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::ScutFerecat);
        let mut app = player_with_loadout(loadout);

        set_player_clip(
            &mut app,
            FighterClip::Hurt,
            FighterClip::Hurt.animation().first,
        );
        app.update();
        set_player_clip(
            &mut app,
            FighterClip::Idle,
            FighterClip::Idle.animation().first,
        );
        app.update();

        let (state, transform) = app
            .world_mut()
            .query_filtered::<(&GearAnimationState, &Transform), With<GearVisualLayer>>()
            .single(app.world())
            .expect("one gear layer exists");
        assert_eq!(
            *state,
            GearAnimationState {
                clip: FighterClip::Idle,
                frame: FighterClip::Idle.animation().first,
            }
        );
        assert_eq!(
            transform.translation.z,
            item_visual(ItemId::ScutFerecat).expect("visual").z_offset,
            "z ordering is preserved while animation state changes"
        );
    }

    #[test]
    fn mirrored_fighters_mirror_gear_attachment_motion() {
        let player = gear_local_transform(
            GearMotion::WeaponHand,
            FighterClip::Attack,
            FighterClip::Attack.animation().first + 1,
            0.06,
            false,
        );
        let enemy = gear_local_transform(
            GearMotion::WeaponHand,
            FighterClip::Attack,
            FighterClip::Attack.animation().first + 1,
            0.06,
            true,
        );
        assert_eq!(player.translation.x, -enemy.translation.x);
        assert_eq!(player.translation.y, enemy.translation.y);
        assert_eq!(player.translation.z, enemy.translation.z);
        assert_ne!(player.rotation, enemy.rotation);
    }

    #[test]
    fn gear_sprite_without_an_asset_server_uses_the_static_placeholder() {
        let visual = item_visual(ItemId::Palos).expect("visual metadata");
        assert_eq!(visual.animated_asset_path, None);
        assert_eq!(visual.fallback_asset_path(), visual.asset_path);
        let sprite = gear_sprite(visual.fallback_asset_path(), None);
        assert_eq!(sprite.custom_size, Some(FIGHTER_SIZE));
    }

    #[test]
    fn bare_player_spawns_no_visual_gear_layers() {
        let mut app = test_app();
        app.update();
        let layers = app
            .world_mut()
            .query_filtered::<(), With<GearVisualLayer>>()
            .iter(app.world())
            .count();
        assert_eq!(layers, 0);
    }

    #[test]
    fn gear_visual_layers_despawn_with_the_arena() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::BataCiobaneasca);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            ArenaPlugin,
            crate::shop::ShopPlugin,
        ));
        app.insert_resource(player_character());
        app.insert_resource(crate::shop::PlayerEquipment(loadout));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<GearVisualLayer>>()
                .iter(app.world())
                .count(),
            1
        );

        leave_fight(&mut app);
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<GearVisualLayer>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
