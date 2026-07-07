//! Arena scene for `GameState::Fight`: placeholder scenery quads, the
//! player's fighter (from [`PlayerCharacter`]) on the left, and the current
//! [`LadderProgress`] opponent from the folklore roster on the right —
//! attributes (lap-scaled), AI profile, and equipment all come from the
//! ladder data. Fighters render as animated sprite-sheet characters (#22,
//! see [`animation`]); they only spawn once their sheets are loaded, which
//! also gates the start of combat (the combat turn waits for the fighters).

pub mod animation;

use bevy::prelude::*;

use crate::character::{Attributes, EnemyFighter, PlayerFighter, spawn_fighter};
use crate::combat::AiProfile;
use crate::core::{GameState, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::items::Equipment;
use crate::menu::CREAM;
use crate::roster::{Boss, LadderProgress};
use animation::{FighterClip, FighterSpriteSheets};

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

// Placeholder scenery palette; real arena backgrounds are a separate issue.
const SKY_COLOR: Color = Color::srgb(0.10, 0.09, 0.16);
const GROUND_COLOR: Color = Color::srgb(0.30, 0.22, 0.14);
const PILLAR_COLOR: Color = Color::srgb(0.45, 0.42, 0.38);

const PILLAR_SIZE: Vec2 = Vec2::new(40.0, 280.0);
const PILLAR_X: f32 = 350.0;

/// Vertical offset of the name label above a fighter's body center.
const LABEL_OFFSET_Y: f32 = FIGHTER_SIZE.y / 2.0 + 24.0;

/// Name-label color for boss opponents; regular fighters use [`CREAM`].
pub const BOSS_LABEL_COLOR: Color = Color::srgb(0.95, 0.45, 0.20);

/// Marker for every arena entity; all of them despawn on
/// `OnExit(GameState::Fight)` via [`despawn_screen`].
#[derive(Component)]
struct ArenaScreen;

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(animation::AnimationPlugin)
            .add_systems(OnEnter(GameState::Fight), spawn_arena)
            .add_systems(
                Update,
                spawn_arena_when_ready.run_if(in_state(GameState::Fight)),
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
    spawn_scene(commands, &player, ladder, &sheets);
}

/// The loading-guard retry: once the sheets finish loading mid-fight-screen,
/// spawns the scene that [`spawn_arena`] skipped.
fn spawn_arena_when_ready(
    commands: Commands,
    player: Option<Res<PlayerCharacter>>,
    ladder: Option<Res<LadderProgress>>,
    sheets: Res<FighterSpriteSheets>,
    asset_server: Option<Res<AssetServer>>,
    spawned: Query<(), With<ArenaScreen>>,
) {
    let Some(player) = player else {
        return; // spawn_arena already warned
    };
    if !spawned.is_empty() || !sheets.ready(asset_server.as_deref()) {
        return;
    }
    spawn_scene(commands, &player, ladder, &sheets);
}

/// Builds the whole fight scene: scenery quads, the player's fighter, and
/// the current ladder opponent (attributes lap-scaled, AI profile and
/// equipment from the roster data; bosses get the [`Boss`] tag and the
/// distinct label color).
fn spawn_scene(
    mut commands: Commands,
    player: &PlayerCharacter,
    ladder: Option<Res<LadderProgress>>,
    sheets: &FighterSpriteSheets,
) {
    let ladder = ladder.map(|ladder| *ladder).unwrap_or_default();
    let opponent = ladder.opponent();
    spawn_scenery(&mut commands);
    spawn_arena_fighter(
        &mut commands,
        player.name.clone(),
        player.attributes,
        PlayerFighter,
        PLAYER_ANCHOR,
        // The player faces right, towards the opponent.
        fighter_sprite(sheets.player.clone(), sheets, false),
        CREAM,
    );
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
        // The opponent faces left, back towards the player.
        fighter_sprite(sheets.opponent(ladder.0), sheets, true),
        if opponent.is_boss {
            BOSS_LABEL_COLOR
        } else {
            CREAM
        },
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

/// Placeholder scenery: sky backdrop, ground strip, and two side pillars,
/// dimensioned relative to the 800x600 logical resolution.
fn spawn_scenery(commands: &mut Commands) {
    commands.spawn((
        ArenaScreen,
        Sprite::from_color(SKY_COLOR, Vec2::new(ARENA_WIDTH, ARENA_HEIGHT)),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));
    commands.spawn((
        ArenaScreen,
        Sprite::from_color(GROUND_COLOR, Vec2::new(ARENA_WIDTH, GROUND_HEIGHT)),
        Transform::from_xyz(0.0, (-ARENA_HEIGHT + GROUND_HEIGHT) / 2.0, -9.0),
    ));
    for x in [-PILLAR_X, PILLAR_X] {
        commands.spawn((
            ArenaScreen,
            Sprite::from_color(PILLAR_COLOR, PILLAR_SIZE),
            Transform::from_xyz(x, GROUND_TOP_Y + PILLAR_SIZE.y / 2.0, -8.0),
        ));
    }
}

/// The animated sprite of one fighter: its sheet over the shared atlas
/// layout, opened on the first idle frame. `flip_x` mirrors the
/// right-facing art for the opponent side.
fn fighter_sprite(sheet: Handle<Image>, sheets: &FighterSpriteSheets, flip_x: bool) -> Sprite {
    Sprite {
        flip_x,
        ..Sprite::from_atlas_image(
            sheet,
            TextureAtlas {
                layout: sheets.layout.clone(),
                index: FighterClip::Idle.animation().first,
            },
        )
    }
}

/// Spawns one fighter through the shared [`spawn_fighter`] (so it carries the
/// #8 components and full pools), then dresses it with the arena visuals: its
/// animated sprite at its anchor (starting on the idle loop) and a
/// world-space name label above (in `label_color`, so bosses read
/// differently at a glance).
fn spawn_arena_fighter(
    commands: &mut Commands,
    name: impl Into<String>,
    attrs: Attributes,
    marker: impl Bundle,
    anchor: Transform,
    sprite: Sprite,
    label_color: Color,
) -> Entity {
    let name = name.into();
    let label = name.clone();
    let fighter = spawn_fighter(commands, name, attrs, marker);
    commands
        .entity(fighter)
        .insert((
            ArenaScreen,
            sprite,
            FighterClip::Idle,
            FighterClip::Idle.animation(),
            anchor,
        ))
        .with_children(|body| {
            body.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(label_color),
                Transform::from_xyz(0.0, LABEL_OFFSET_Y, 0.1),
            ));
        });
    fighter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Fighter, FighterName, Health, Stamina, stats};
    use crate::core::CorePlugin;
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
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(player_character());
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
        assert_eq!(scenery, 4, "sky + ground + two pillars");
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
}
