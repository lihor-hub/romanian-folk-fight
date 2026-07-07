//! Arena scene for `GameState::Fight`: placeholder scenery quads, the
//! player's fighter (from [`PlayerCharacter`]) on the left, and a hardcoded
//! opponent on the right. Later issues (combat engine, HUD, sprites) build on
//! this scene; the anchor constants are reused by the animation work.

use bevy::prelude::*;

use crate::character::{Attributes, EnemyFighter, PlayerFighter, spawn_fighter};
use crate::combat::AiProfile;
use crate::core::{GameState, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::menu::CREAM;

/// Logical resolution the scene is designed for (the window in `main.rs`).
pub const ARENA_WIDTH: f32 = 800.0;
/// Logical height matching [`ARENA_WIDTH`].
pub const ARENA_HEIGHT: f32 = 600.0;

/// Size of a placeholder fighter body quad.
pub const FIGHTER_SIZE: Vec2 = Vec2::new(60.0, 120.0);

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

/// The hardcoded Phase 1 opponent; the opponent ladder (Phase 3) replaces it.
const ENEMY_NAME: &str = "Strigoi";
const ENEMY_ATTRIBUTES: Attributes = Attributes {
    putere: 2,
    agilitate: 2,
    vitalitate: 2,
    noroc: 1,
};

// Placeholder palette; real backgrounds and sprites arrive in Phase 4.
const SKY_COLOR: Color = Color::srgb(0.10, 0.09, 0.16);
const GROUND_COLOR: Color = Color::srgb(0.30, 0.22, 0.14);
const PILLAR_COLOR: Color = Color::srgb(0.45, 0.42, 0.38);
const PLAYER_COLOR: Color = Color::srgb(0.75, 0.20, 0.15);
const ENEMY_COLOR: Color = Color::srgb(0.55, 0.50, 0.65);

const PILLAR_SIZE: Vec2 = Vec2::new(40.0, 280.0);
const PILLAR_X: f32 = 350.0;

/// Vertical offset of the name label above a fighter's body center.
const LABEL_OFFSET_Y: f32 = FIGHTER_SIZE.y / 2.0 + 24.0;

/// Marker for every arena entity; all of them despawn on
/// `OnExit(GameState::Fight)` via [`despawn_screen`].
#[derive(Component)]
struct ArenaScreen;

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Fight), spawn_arena)
            .add_systems(OnExit(GameState::Fight), despawn_screen::<ArenaScreen>);
    }
}

/// Builds the whole fight scene: scenery quads plus the two fighters at their
/// anchors. Skips (with a warning) if no [`PlayerCharacter`] was confirmed,
/// which only happens if the state flow is driven out of order.
fn spawn_arena(mut commands: Commands, player: Option<Res<PlayerCharacter>>) {
    let Some(player) = player else {
        warn!("entered GameState::Fight without a PlayerCharacter; arena not spawned");
        return;
    };
    spawn_scenery(&mut commands);
    spawn_arena_fighter(
        &mut commands,
        player.name.clone(),
        player.attributes,
        PlayerFighter,
        PLAYER_ANCHOR,
        PLAYER_COLOR,
        // The player faces right, towards the opponent.
        false,
    );
    spawn_arena_fighter(
        &mut commands,
        ENEMY_NAME,
        ENEMY_ATTRIBUTES,
        // The Strigoi fights with the default balanced aggression; the
        // opponent ladder issue tunes profiles per archetype.
        (EnemyFighter, AiProfile::default()),
        ENEMY_ANCHOR,
        ENEMY_COLOR,
        // The opponent faces left, back towards the player.
        true,
    );
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

/// Spawns one fighter through the shared [`spawn_fighter`] (so it carries the
/// #8 components and full pools), then dresses it with the arena visuals: a
/// colored body quad at its anchor and a world-space name label above.
fn spawn_arena_fighter(
    commands: &mut Commands,
    name: impl Into<String>,
    attrs: Attributes,
    marker: impl Bundle,
    anchor: Transform,
    color: Color,
    flip_x: bool,
) {
    let name = name.into();
    let label = name.clone();
    let fighter = spawn_fighter(commands, name, attrs, marker);
    commands
        .entity(fighter)
        .insert((
            ArenaScreen,
            Sprite {
                flip_x,
                ..Sprite::from_color(color, FIGHTER_SIZE)
            },
            anchor,
        ))
        .with_children(|body| {
            body.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(CREAM),
                Transform::from_xyz(0.0, LABEL_OFFSET_Y, 0.1),
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Fighter, FighterName, Health, Stamina, stats};
    use crate::core::CorePlugin;
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

    /// Headless app already inside the fight arena.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ArenaPlugin));
        app.insert_resource(player_character());
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
            "exactly one Strigoi (2/2/2/1) at full pools"
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
    fn the_enemy_carries_the_default_ai_profile_and_the_player_none() {
        let mut app = test_app();
        let profile = app
            .world_mut()
            .query_filtered::<&AiProfile, With<EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter has an AI profile");
        assert_eq!(*profile, AiProfile::default(), "balanced 0.5 aggression");
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
            vec!["Făt-Frumos".to_string(), "Strigoi".to_string()]
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
