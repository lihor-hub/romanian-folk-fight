//! Arena scene for `GameState::Fight`: placeholder scenery quads, the
//! player's fighter (from [`PlayerCharacter`]) on the left, and the current
//! [`LadderProgress`] opponent from the folklore roster on the right —
//! attributes (lap-scaled), AI profile, and equipment all come from the
//! ladder data. Fighters render as animated sprite-sheet characters (#22,
//! see [`animation`]); they only spawn once their sheets are loaded, which
//! also gates the start of combat (the combat turn waits for the fighters).

pub mod animation;
pub mod fx;

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::character::{
    Attributes, CatalogError, CharacterCatalog, CharacterDefinition, EnemyFighter, GenerationError,
    PlayerFighter, bundled_human_catalog, fallback_human, spawn_fighter,
};
use crate::combat::AiProfile;
use crate::core::{GameState, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::cutout::{
    CutoutPartMarker, CutoutRig, CutoutRigTemplate, CutoutTemplate, GearVisualLayer, boss_template,
    cutout_rig_owner, enemy_template, human_template, spawn_character_definition_rig,
    spawn_cutout_rig_with_accent, spawn_gear_attachment_layers,
};
use crate::items::{Equipment, GearMotion};
use crate::roster::{Boss, CampaignSeed, LadderProgress, Opponent, PreparedEncounter};
use crate::theme::GROUND_COLOR;
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

#[derive(SystemParam)]
struct EncounterResources<'w> {
    ladder: Option<Res<'w, LadderProgress>>,
    campaign_seed: Option<Res<'w, CampaignSeed>>,
    prepared: Option<Res<'w, PreparedEncounter>>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ArenaSet {
    GearRefresh,
}

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
                    spawn_equipped_gear_layers.in_set(ArenaSet::GearRefresh),
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
    encounter: EncounterResources,
    sheets: Res<FighterSpriteSheets>,
    backgrounds: Res<ArenaBackgrounds>,
    asset_server: Option<Res<AssetServer>>,
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
        encounter.ladder,
        encounter
            .campaign_seed
            .map(|seed| *seed)
            .unwrap_or_default(),
        encounter.prepared.as_deref(),
        &backgrounds,
        asset_server.as_deref(),
    );
}

/// The loading-guard retry: once the sheets finish loading mid-fight-screen,
/// spawns the scene that [`spawn_arena`] skipped.
fn spawn_arena_when_ready(
    commands: Commands,
    player: Option<Res<PlayerCharacter>>,
    encounter: EncounterResources,
    sheets: Res<FighterSpriteSheets>,
    backgrounds: Res<ArenaBackgrounds>,
    asset_server: Option<Res<AssetServer>>,
    spawned: Query<(), With<ArenaScreen>>,
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
        encounter.ladder,
        encounter
            .campaign_seed
            .map(|seed| *seed)
            .unwrap_or_default(),
        encounter.prepared.as_deref(),
        &backgrounds,
        asset_server.as_deref(),
    );
}

/// Builds the whole fight scene: scenery quads, the player's fighter, and
/// the current ladder opponent (attributes lap-scaled, AI profile and
/// equipment from the roster data; bosses get the [`Boss`] tag).
fn spawn_scene(
    mut commands: Commands,
    player: &PlayerCharacter,
    ladder: Option<Res<LadderProgress>>,
    campaign_seed: CampaignSeed,
    prepared_encounter: Option<&PreparedEncounter>,
    backgrounds: &ArenaBackgrounds,
    asset_server: Option<&AssetServer>,
) {
    let ladder = ladder.map(|ladder| *ladder).unwrap_or_default();
    let opponent = ladder.opponent();
    let seeded_visual = prepared_encounter
        .filter(|prepared| {
            ladder.0.is_multiple_of(crate::roster::LADDER.len())
                && prepared.0.encounter_id == crate::roster::HOT_DE_CODRU_ENCOUNTER_ID
        })
        .map(|prepared| SeededEncounterVisual::Generated(prepared.0.clone()))
        .unwrap_or_else(|| match ladder.seeded_opponent(campaign_seed) {
            Some(generated) => seeded_encounter_visual(generated, bundled_human_catalog()),
            None => SeededEncounterVisual::Legacy,
        });
    spawn_background(&mut commands, backgrounds, background_tier(ladder));
    spawn_scenery(&mut commands);
    let player_fighter = spawn_arena_fighter(
        &mut commands,
        player.name.clone(),
        player.attributes,
        PlayerFighter,
        PLAYER_ANCHOR,
        ArenaRig::Character(&player.definition),
        false,
        asset_server,
        None,
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
        match &seeded_visual {
            SeededEncounterVisual::Generated(generated) => {
                ArenaRig::Character(&generated.definition)
            }
            SeededEncounterVisual::KnownGood(definition) => ArenaRig::Character(definition),
            SeededEncounterVisual::Legacy => ArenaRig::Template(opponent_template(opponent)),
        },
        true,
        asset_server,
        Some(opponent.accent_hue),
    );
    let mut equipment = Equipment::default();
    for &id in opponent.equipment {
        equipment.equip(id);
    }
    commands.entity(enemy).insert(equipment);
    if let SeededEncounterVisual::Generated(generated) = seeded_visual {
        commands.entity(enemy).insert(generated);
    }
    if opponent.is_boss {
        commands.entity(enemy).insert(Boss {
            intro_line: opponent.intro_line,
        });
    }
}

enum SeededEncounterVisual {
    Generated(crate::roster::SeededOpponent),
    KnownGood(CharacterDefinition),
    Legacy,
}

fn seeded_encounter_visual(
    generated: Result<crate::roster::SeededOpponent, GenerationError>,
    catalog: Result<&CharacterCatalog, CatalogError>,
) -> SeededEncounterVisual {
    match generated {
        Ok(generated) => SeededEncounterVisual::Generated(generated),
        Err(generation_error) => {
            warn!(
                "could not generate seeded opponent ({generation_error}); trying the versioned \
                 known-good modular human"
            );
            let fallback = match catalog {
                Ok(catalog) => fallback_human(catalog).map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            match fallback {
                Ok(definition) => SeededEncounterVisual::KnownGood(definition),
                Err(fallback_error) => {
                    error!(
                        "known-good modular human also failed ({fallback_error}); using the legacy \
                         authored opponent template"
                    );
                    SeededEncounterVisual::Legacy
                }
            }
        }
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
/// animated sprite at its anchor (starting on the idle loop). `accent_hue`
/// (#118) is the opponent's one muted accent color, tinted onto the rig's
/// clothing/accent parts; the player is always spawned with `None` so its
/// template stays untinted.
// Each argument is one distinct piece of the fighter's dressing; bundling
// them into a struct for one call site would only add indirection.
#[allow(clippy::too_many_arguments)]
fn spawn_arena_fighter<'a>(
    commands: &mut Commands,
    name: impl Into<String>,
    attrs: Attributes,
    marker: impl Bundle,
    anchor: Transform,
    rig: ArenaRig<'a>,
    flip_x: bool,
    asset_server: Option<&AssetServer>,
    accent_hue: Option<Color>,
) -> Entity {
    let name = name.into();
    let fighter = spawn_fighter(commands, name, attrs, marker);
    commands.entity(fighter).insert((
        ArenaScreen,
        FighterClip::Idle,
        FighterClip::Idle.animation(),
        anchor,
    ));
    match rig {
        ArenaRig::Character(definition) => spawn_character_definition_rig(
            commands,
            fighter,
            definition,
            asset_server,
            flip_x,
            None,
            accent_hue,
        ),
        ArenaRig::Template(template) => spawn_cutout_rig_with_accent(
            commands,
            fighter,
            template,
            asset_server,
            flip_x,
            accent_hue,
        ),
    }
    fighter
}

enum ArenaRig<'a> {
    Character(&'a CharacterDefinition),
    Template(CutoutRigTemplate),
}

fn opponent_template(opponent: &Opponent) -> CutoutRigTemplate {
    match opponent.cutout_template {
        CutoutTemplate::Human => human_template(),
        CutoutTemplate::Enemy => enemy_template(),
        CutoutTemplate::Boss => boss_template(),
    }
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

/// Query alias for fighter roots read by [`sync_gear_visual_layers`].
/// `Without<GearVisualLayer>` keeps this read disjoint from that system's
/// `&mut Sprite` gear-layer access (Bevy error B0001): fighter roots are
/// never gear layers.
type FighterAnimationSources<'w, 's> = Query<
    'w,
    's,
    (
        &'static FighterClip,
        &'static animation::SpriteAnimation,
        Option<&'static Sprite>,
        Option<&'static CutoutRig>,
    ),
    Without<GearVisualLayer>,
>;

/// Spawns gear overlays from each fighter's current equipment.
/// The shop copies [`crate::shop::PlayerEquipment`] onto the player fighter
/// after spawn, and roster opponents also carry [`Equipment`]. This system
/// reacts to both.
///
/// Weapons/shields/boots attach to hands, forearms, and feet, which are now
/// nested several joints deep under their own parent part rather than being
/// direct children of the fighter root (#117); ownership is resolved by
/// climbing the chain via [`cutout_rig_owner`] instead of assuming a single
/// `ChildOf` hop from the part to the fighter.
fn spawn_equipped_gear_layers(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    fighters: ChangedFighterEquipment,
    parts: Query<(Entity, &CutoutPartMarker, &ChildOf)>,
    existing_layers: Query<(Entity, &ChildOf), With<GearVisualLayer>>,
) {
    let parent_of = |entity: Entity| {
        parts
            .get(entity)
            .ok()
            .map(|(_, _, child_of)| child_of.parent())
    };
    for (fighter, equipment) in &fighters {
        for (layer, child_of) in &existing_layers {
            if cutout_rig_owner(child_of.parent(), parent_of) == fighter {
                commands.entity(layer).despawn();
            }
        }

        spawn_gear_attachment_layers(
            &mut commands,
            equipment,
            asset_server.as_deref(),
            |wanted| {
                parts
                    .iter()
                    .find(|(part, marker, _)| {
                        marker.kind == wanted && cutout_rig_owner(*part, parent_of) == fighter
                    })
                    .map(|(part, _, _)| part)
            },
            |_| GearAnimationState {
                clip: FighterClip::Idle,
                frame: FighterClip::Idle.animation().first,
            },
        );
    }
}

/// Copies the owning fighter's current animation state into every gear layer
/// and applies a small local transform so static overlay art feels attached
/// to hands, shield arm, head, torso, or feet during each clip.
///
/// Weapons/shields/boots attach to hands, forearms, and feet, which are now
/// nested several joints deep under their own parent part rather than being
/// direct children of the fighter root (#117); ownership is resolved by
/// climbing the chain via [`cutout_rig_owner`] instead of assuming a single
/// `ChildOf` hop from the part to the fighter.
fn sync_gear_visual_layers(
    fighters: FighterAnimationSources,
    parts: Query<&ChildOf, With<CutoutPartMarker>>,
    mut layers: Query<(
        &GearVisualLayer,
        &ChildOf,
        &mut GearAnimationState,
        &mut Transform,
        &mut Sprite,
    )>,
) {
    let parent_of = |entity: Entity| parts.get(entity).ok().map(|child_of| child_of.parent());
    for (layer, child_of, mut state, mut transform, mut gear_sprite) in &mut layers {
        let root = cutout_rig_owner(child_of.parent(), parent_of);
        let Ok((clip, anim, sprite, rig)) = fighters.get(root) else {
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
        // Mirror the artwork itself, not only the attachment transform, so
        // an equipped weapon/shield faces the same way as its fighter.
        gear_sprite.flip_x = flip_x;
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
        CutoutPartKind, CutoutPartMarker, CutoutRig, CutoutTemplate, boss_template, enemy_template,
        gear_sprite, human_template, human_template_for,
    };
    use crate::items::{ItemId, Slot, item_visual};
    use crate::roster::{CampaignSeed, LADDER, PreparedEncounter, SeededOpponent};
    use bevy::state::app::StatesPlugin;

    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
        atac: 1,
        aparare: 2,
        carisma: 1,
        magie: 0,
    };

    fn player_character() -> PlayerCharacter {
        PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
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

    fn test_app_at_with_campaign_seed(progress: LadderProgress, campaign_seed: u64) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(player_character());
        app.insert_resource(progress);
        app.insert_resource(CampaignSeed(campaign_seed));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        app
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

    /// Recursively counts every `CutoutPartMarker` entity under `root`, at
    /// any depth. Forearms/hands/shins/feet are nested several joints deep
    /// rather than being direct children of the fighter root (#117), so
    /// this walks the whole subtree instead of only `root`'s immediate
    /// `Children`.
    fn cutout_child_count(app: &mut App, root: Entity) -> usize {
        let world = app.world();
        let mut count = 0;
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if world.get::<CutoutPartMarker>(child).is_some() {
                    count += 1;
                }
                stack.push(child);
            }
        }
        count
    }

    /// Maps every `CutoutPartMarker` entity to its immediate `ChildOf`
    /// parent, so [`cutout_rig_owner`] can climb from any body part (or the
    /// part a gear layer is parented under) up to the owning fighter --
    /// parts several joints deep (forearms, hands, shins, feet) are no
    /// longer direct children of the fighter root (#117).
    fn cutout_part_parents(app: &mut App) -> std::collections::HashMap<Entity, Entity> {
        app.world_mut()
            .query::<(Entity, &CutoutPartMarker, &ChildOf)>()
            .iter(app.world())
            .map(|(part, _, child_of)| (part, child_of.parent()))
            .collect()
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

    /// Maps every `CutoutPartMarker` entity under `root`, at any depth, to
    /// its rendered `Sprite::color`. Forearms/hands/shins/feet nest several
    /// joints deep (#117), so this walks the whole subtree rather than only
    /// `root`'s immediate `Children`.
    fn part_colors_by_kind(
        app: &mut App,
        root: Entity,
    ) -> std::collections::HashMap<CutoutPartKind, Color> {
        let world = app.world();
        let mut colors = std::collections::HashMap::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if let Some(marker) = world.get::<CutoutPartMarker>(child)
                    && let Some(sprite) = world.get::<Sprite>(child)
                {
                    colors.insert(marker.kind, sprite.color);
                }
                stack.push(child);
            }
        }
        colors
    }

    #[test]
    fn the_hot_de_codru_carries_an_accent_tint_the_player_does_not() {
        // #118: Hoț de codru (LADDER[0], the fight the default LadderProgress
        // starts on) used to spawn from the exact same untinted human
        // template as the player -- pixel-identical fighters. At least one
        // clothing/accent part must now render with a different
        // `Sprite::color` than the player's corresponding part.
        let mut app = test_app();
        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");

        let player_colors = part_colors_by_kind(&mut app, player);
        let enemy_colors = part_colors_by_kind(&mut app, enemy);

        let differing = enemy_colors.iter().any(|(kind, enemy_color)| {
            player_colors
                .get(kind)
                .is_some_and(|player_color| player_color != enemy_color)
        });
        assert!(
            differing,
            "Hoț de codru should render at least one body part with a Sprite::color \
             different from the player's corresponding part"
        );
    }

    #[test]
    fn arena_fighter_body_part_sprites_flip_to_match_the_rig() {
        let mut app = test_app();

        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");

        let part_parents = cutout_part_parents(&mut app);

        let mut player_parts = 0;
        let mut enemy_parts = 0;
        let mut query = app.world_mut().query::<(Entity, &Sprite)>();
        for (entity, sprite) in query.iter(app.world()) {
            if !part_parents.contains_key(&entity) {
                continue;
            }
            let owner = cutout_rig_owner(entity, |e| part_parents.get(&e).copied());
            if owner == player {
                player_parts += 1;
                assert!(!sprite.flip_x, "player body part sprites stay unflipped");
            } else if owner == enemy {
                enemy_parts += 1;
                assert!(sprite.flip_x, "enemy body part sprites mirror the artwork");
            }
        }
        assert!(player_parts > 0, "player has body-part sprites");
        assert!(enemy_parts > 0, "enemy has body-part sprites");
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
                definition: crate::character::CharacterDefinition::legacy_human(appearance),
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
    fn arena_player_rig_uses_the_persisted_definition_as_visual_authority() {
        let legacy_projection = PlayerAppearance::default();
        let definition_appearance = PlayerAppearance {
            skin_tone: SkinTone::Deep,
            build: BodyBuild::Powerful,
            hair: HairStyle::Tied,
            accent: AccentColor::Storm,
        };
        let mut app = test_app_with(
            PlayerCharacter {
                name: "Făt-Frumos".to_string(),
                attributes: PLAYER_ATTRIBUTES,
                appearance: legacy_projection,
                definition: crate::character::CharacterDefinition::legacy_human(
                    definition_appearance,
                ),
            },
            LadderProgress::default(),
        );

        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        let expected_torso = human_template_for(definition_appearance)
            .parts
            .into_iter()
            .find(|part| part.kind == CutoutPartKind::Torso)
            .expect("human template has a torso");
        let part_parents = cutout_part_parents(&mut app);
        let torso_size = app
            .world_mut()
            .query::<(
                Entity,
                &CutoutPartMarker,
                &crate::cutout::CutoutPartRestPose,
            )>()
            .iter(app.world())
            .find_map(|(entity, marker, rest)| {
                (marker.kind == CutoutPartKind::Torso
                    && cutout_rig_owner(entity, |part| part_parents.get(&part).copied()) == player)
                    .then_some(rest.size)
            })
            .expect("player rig has a torso");

        assert_eq!(
            torso_size, expected_torso.size,
            "the persisted definition, not its legacy projection, drives the arena rig"
        );
    }

    #[test]
    fn arena_invalid_persisted_part_id_renders_known_good_without_mutating_the_save() {
        let mut player = player_character();
        player.definition.parts.hair =
            crate::character::PartId::new("human.hair.missing.v1").unwrap();
        let persisted_definition = player.definition.clone();
        let mut app = test_app_with(player, LadderProgress::default());

        assert_eq!(
            app.world().resource::<PlayerCharacter>().definition,
            persisted_definition,
            "render fallback must not rewrite the saved identity"
        );
        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter still spawns");
        let part_parents = cutout_part_parents(&mut app);
        let rendered_hair_id = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker)>()
            .iter(app.world())
            .find_map(|(entity, marker)| {
                (marker.kind == CutoutPartKind::Hair
                    && cutout_rig_owner(entity, |part| part_parents.get(&part).copied()) == player)
                    .then(|| marker.source_id.clone())
                    .flatten()
            })
            .expect("known-good fallback renders a catalog-backed hair part");

        assert_eq!(rendered_hair_id.as_str(), "human.hair.braided.v1");
    }

    #[test]
    fn arena_spawns_no_floating_name_labels() {
        let mut app = test_app();
        let labels = app.world_mut().query::<&Text2d>().iter(app.world()).count();
        assert_eq!(labels, 0, "no floating Text2d name labels in the arena");
    }

    #[test]
    fn seeded_human_encounter_repeats_and_an_alternate_seed_changes_only_unlocked_choices() {
        fn generated(app: &mut App) -> SeededOpponent {
            app.world_mut()
                .query_filtered::<&SeededOpponent, With<EnemyFighter>>()
                .single(app.world())
                .expect("the representative human opponent is generated")
                .clone()
        }

        let mut first_entry = test_app_at_with_campaign_seed(LadderProgress(0), 0);
        let mut repeated_entry = test_app_at_with_campaign_seed(LadderProgress(0), 0);
        let mut alternate_entry = test_app_at_with_campaign_seed(LadderProgress(0), 1);

        let first = generated(&mut first_entry);
        let repeated = generated(&mut repeated_entry);
        let alternate = generated(&mut alternate_entry);

        assert_eq!(first, repeated, "the same encounter seed repeats exactly");
        assert_ne!(first.seed, alternate.seed);
        assert_ne!(
            first.definition.parts.hair, alternate.definition.parts.hair,
            "the pinned alternate review seed exercises the unlocked hair choice"
        );
        assert_eq!(first.definition.parts.body, alternate.definition.parts.body);
        assert_eq!(first.definition.parts.face, alternate.definition.parts.face);
        assert_eq!(
            first.definition.parts.torso,
            alternate.definition.parts.torso
        );
        assert_eq!(first.definition.parts.legs, alternate.definition.parts.legs);
        assert_eq!(first.definition.parts.feet, alternate.definition.parts.feet);
        assert_eq!(
            first.definition.parts.waist,
            alternate.definition.parts.waist
        );
        assert_eq!(
            first.definition.parts.accessories,
            alternate.definition.parts.accessories
        );
        assert_eq!(first.definition.appearance, alternate.definition.appearance);

        let mut other_human = test_app_at_with_campaign_seed(LadderProgress(6), 0);
        assert!(
            other_human
                .world_mut()
                .query_filtered::<&SeededOpponent, With<EnemyFighter>>()
                .single(other_human.world())
                .is_err(),
            "Solomonar keeps the existing human template path"
        );
    }

    #[test]
    fn arena_uses_the_prepared_persisted_encounter_instead_of_regenerating_it() {
        let mut persisted = LadderProgress(0)
            .seeded_opponent(CampaignSeed::default())
            .expect("the representative encounter is generated")
            .expect("the bundled profile resolves");
        persisted.definition.parts.hair =
            crate::character::PartId::new("human.hair.long.v1").expect("catalog ID is valid");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(player_character());
        app.insert_resource(LadderProgress(0));
        app.insert_resource(CampaignSeed::default());
        app.insert_resource(PreparedEncounter(persisted.clone()));
        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();

        let combat_identity = app
            .world_mut()
            .query_filtered::<&SeededOpponent, With<EnemyFighter>>()
            .single(app.world())
            .expect("the seeded enemy carries its exact combat identity");
        assert_eq!(combat_identity, &persisted);
    }

    #[test]
    fn generation_error_uses_known_good_modular_human_before_legacy_template() {
        let catalog =
            crate::character::bundled_human_catalog().expect("bundled catalog is available");
        let choice = seeded_encounter_visual(
            Err(crate::character::GenerationError::MissingRequiredSlot {
                region: crate::character::BodyRegion::Hair,
            }),
            Ok(catalog),
        );

        let SeededEncounterVisual::KnownGood(definition) = choice else {
            panic!("GenerationError must choose the shared known-good modular human first");
        };
        assert_eq!(definition.parts, catalog.known_good_human().clone());
    }

    /// The enemy's `(FighterName, Attributes, Equipment, Option<Boss>)`.
    fn enemy_snapshot(app: &mut App) -> (String, Attributes, Equipment, Option<Boss>) {
        let (_entity, name, attrs, equipment, boss) = app
            .world_mut()
            .query_filtered::<(Entity, &FighterName, &Attributes, &Equipment, Option<&Boss>), With<EnemyFighter>>()
            .single(app.world())
            .expect("exactly one enemy fighter");
        (name.0.clone(), *attrs, equipment.clone(), boss.copied())
    }

    #[test]
    fn the_ladder_position_picks_the_spawned_opponent() {
        // LadderProgress(4) is the acceptance-criteria fight: Muma Pădurii,
        // the first boss.
        let mut app = test_app_at(LadderProgress(4));
        let (name, attrs, _, boss) = enemy_snapshot(&mut app);
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
        let (name, _, _, boss) = enemy_snapshot(&mut app);
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
    fn a_regular_opponent_carries_no_boss_tag() {
        let mut app = test_app();
        let (name, _, equipment, boss) = enemy_snapshot(&mut app);
        assert_eq!(name, "Hoț de codru");
        assert_eq!(boss, None);
        assert_eq!(equipment, Equipment::default(), "the Hoț fights bare");
    }

    #[test]
    fn equipped_opponents_spawn_with_their_roster_gear() {
        let mut app = test_app_at(LadderProgress(9));
        let (name, _, equipment, boss) = enemy_snapshot(&mut app);
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
        let part_parents = cutout_part_parents(&mut app);
        let mut layers: Vec<(ItemId, Slot, GearMotion, Entity)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let owner = cutout_rig_owner(child_of.parent(), |e| part_parents.get(&e).copied());
                (owner == enemy).then_some((layer.item, layer.slot, layer.motion, owner))
            })
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
    fn equipped_opponent_gear_layer_sprites_mirror_with_the_rig() {
        let mut app = test_app_at(LadderProgress(9));
        app.update();

        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        let part_parents = cutout_part_parents(&mut app);

        let mut checked = 0;
        let mut query = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf, &Sprite)>();
        for (_, child_of, sprite) in query.iter(app.world()) {
            let owner = cutout_rig_owner(child_of.parent(), |e| part_parents.get(&e).copied());
            if owner == enemy {
                checked += 1;
                assert!(sprite.flip_x, "enemy gear layers mirror with the rig");
            }
        }
        assert!(checked > 0, "the boss enemy has gear layers to check");
    }

    #[test]
    fn equipped_player_gear_layer_sprites_stay_unflipped() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::ScutFerecat);

        let mut app = player_with_loadout(loadout);

        let mut checked = 0;
        let mut query = app.world_mut().query::<(&GearVisualLayer, &Sprite)>();
        for (_, sprite) in query.iter(app.world()) {
            checked += 1;
            assert!(!sprite.flip_x, "player gear layers stay unflipped");
        }
        assert!(checked > 0, "the player has gear layers to check");
    }

    #[test]
    fn a_second_lap_opponent_is_stronger_and_labeled_with_the_lap() {
        let mut app = test_app_at(LadderProgress(10));
        let (name, attrs, _, boss) = enemy_snapshot(&mut app);
        assert_eq!(name, "Hoț de codru (Turul 2)");
        assert_eq!(boss, None);
        use crate::roster::attribute_total;
        let total = attribute_total(&attrs);
        assert_eq!(total, 14, "total 12 scaled by 1.2 rounds to 14");
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
        let part_parents = cutout_part_parents(&mut app);
        let mut layers: Vec<(ItemId, Slot, Entity)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let owner = cutout_rig_owner(child_of.parent(), |e| part_parents.get(&e).copied());
                (owner == player).then_some((layer.item, layer.slot, owner))
            })
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

    #[test]
    fn equipped_player_gear_layers_attach_to_matching_cutout_parts() {
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::ScutFerecat);
        loadout.equip(ItemId::CaciulaDeOaie);
        loadout.equip(ItemId::CizmeDeVoinic);

        let mut app = player_with_loadout(loadout);
        app.update();

        let part_kinds: Vec<(Entity, CutoutPartKind)> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker)>()
            .iter(app.world())
            .map(|(part, marker)| (part, marker.kind))
            .collect();
        let mut attachments: Vec<(Slot, CutoutPartKind)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .map(|(layer, child_of)| {
                let kind = part_kinds
                    .iter()
                    .find(|(part, _)| *part == child_of.parent())
                    .map(|(_, kind)| *kind)
                    .expect("gear layer parent is a cutout body part");
                (layer.slot, kind)
            })
            .collect();
        attachments.sort_by_key(|(slot, _)| *slot as usize);

        assert_eq!(
            attachments,
            vec![
                (Slot::Weapon, CutoutPartKind::HandFront),
                (Slot::Shield, CutoutPartKind::ForearmBack),
                (Slot::Head, CutoutPartKind::Head),
                (Slot::Feet, CutoutPartKind::FootBack),
                (Slot::Feet, CutoutPartKind::FootFront),
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
        let sprite = gear_sprite(visual, None);
        assert_eq!(sprite.custom_size, Some(Vec2::new(28.0, 92.0)));
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
