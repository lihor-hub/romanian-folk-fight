//! Desktop combat action palette (#189, a child of #143): renders the seven
//! current combat actions — plus whatever [`super::actions::ExtraDescriptors`]
//! a test registers — entirely from [`ActionDescriptor`]s. `hud::spawn_hud`
//! delegates the action-bar subtree to [`spawn_action_bar`] below instead of
//! hard-coding a seven-button `children![...]` list; every subsequent frame,
//! [`update_action_buttons`] re-derives the same descriptors from live duel
//! state and reconciles the already-spawned buttons against them.
//!
//! Mobile's 2×2 grid sizing (`is_mobile` branches throughout this module) is
//! carried over unchanged from the pre-#189 HUD — phone category disclosure
//! is a later #143 child (#199/#213), out of scope here. What #189 actually
//! changes is that the *set* of buttons (their count, order, and content)
//! always comes from [`super::actions::generate_action_descriptors`] plus
//! [`super::actions::ExtraDescriptors`], never a fixed list of match arms —
//! see the `extensibility_seam` test module below for the proof.

use bevy::prelude::*;

use crate::character::{Attributes, EnemyFighter, PlayerFighter, Stamina};
use crate::core::UiFont;
use crate::menu::DisabledButton;
use crate::theme::{
    ACTION_BUTTON_TOUCH_TARGET, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED,
    CREAM, GOLD, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};

use super::actions::{
    ActionDescriptor, ActionId, DescriptorContext, ExtraDescriptors, generate_action_descriptors,
};
use super::engine::CombatAction;
use super::hud::ActionBarRoot;
use super::systems::{CombatPresentation, CombatTurn, PlayerActionEvent};

#[cfg(test)]
use crate::theme::PANEL_BORDER_INSET;

const ACTION_BUTTON_WIDTH: f32 = 100.0;
const ACTION_BUTTON_HEIGHT: f32 = 64.0;
// Narrowed from 6.0 (#120): `panel_bundle` now floors this bar's padding at
// `PANEL_BORDER_INSET` (24px, up from the 8px below), so the desktop strip
// needs the extra ~4px of the 7-button row back to still fit
// `HUD_TARGET_WIDTH`; see `desktop_action_strip_occupied_width`.
pub(super) const ACTION_BAR_DESKTOP_GAP: f32 = 5.0;
const ACTION_BAR_PADDING: f32 = 8.0;
const ACTION_BAR_DESKTOP_INSET: f32 = 10.0;
#[cfg(test)]
const HUD_TARGET_WIDTH: f32 = 800.0;
#[cfg(test)]
const ACTION_BUTTON_COUNT: f32 = 7.0;

/// The combat action a HUD button submits when clicked, plus the stable
/// descriptor id it was built from — id-keyed (not action-keyed) lookup so
/// two descriptors can in principle share the same [`CombatAction`] intent
/// (the extensibility test's eighth descriptor does exactly this) without
/// becoming ambiguous.
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's `fight-palette-desktop`
/// browser scenario (#189) reads this marker (plus each button's real
/// `ComputedNode`/`UiGlobalTransform`) to publish an exact geometric
/// "every button rendered inside the letterboxed stage rect" fact, computed
/// once in native Bevy space rather than duplicated pixel-math on the
/// browser-harness side.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionButton {
    pub id: ActionId,
    pub intent: CombatAction,
}

/// Small glyph at the head of an action tile; stable so tests can confirm
/// buttons are icon-led without depending on screenshots. Purely cosmetic —
/// no payload — since #122's real pictogram art will key off
/// [`ActionDescriptor::pictogram_id`] directly rather than this marker.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionGlyph;

/// Marker for the text node that shows an action's cost line when enabled
/// and its disabled reason when it isn't (#189's "expose their reason"
/// acceptance criterion) — the same text slot, never a new UI element.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionCostOrReason;

/// The bottom action bar with combat and movement buttons: a single
/// (wrapping) row on desktop, a tighter wrap grid of ≥48px touch targets
/// under the mobile breakpoint (#31). Iterates
/// [`generate_action_descriptors`] plus [`ExtraDescriptors`] — never a
/// hard-coded seven-button `children![...]` list — so a later registered
/// action renders here with no edits to this function.
///
/// Spawned with [`DescriptorContext::spawn_placeholder`] (see its docs):
/// `CombatTurn` does not exist yet at this point in the `OnEnter(Fight)`
/// schedule, so every button spawns showing its cost line, uncolored as
/// disabled; [`update_action_buttons`] corrects colors and text against real
/// state on the very next frame, exactly like the pre-#189 HUD did for
/// button color alone.
pub(super) fn spawn_action_bar(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
    extra: &ExtraDescriptors,
) {
    let node = if is_mobile {
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(8.0),
            right: Val::Px(8.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(ACTION_BAR_PADDING)),
            ..default()
        }
    } else {
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(ACTION_BAR_DESKTOP_INSET),
            right: Val::Px(ACTION_BAR_DESKTOP_INSET),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(0.0),
            column_gap: Val::Px(ACTION_BAR_DESKTOP_GAP),
            padding: UiRect::all(Val::Px(ACTION_BAR_PADDING)),
            ..default()
        }
    };

    let mut descriptors = generate_action_descriptors(&DescriptorContext::spawn_placeholder());
    descriptors.extend(extra.0.iter().cloned());

    parent
        .spawn((
            panel_bundle(panel_texture, node),
            BackgroundColor(PANEL_LINEN),
            ActionBarRoot,
        ))
        .with_children(|bar| {
            for descriptor in &descriptors {
                spawn_action_button(bar, descriptor, ui_font, is_mobile);
            }
        });
}

/// One action button: the Romanian label over its cost/disabled-reason line.
/// Under the mobile breakpoint it grows to a ≥[`ACTION_BUTTON_TOUCH_TARGET`]
/// square-ish tile so four of them wrap into a 2×2 grid.
fn spawn_action_button(
    parent: &mut ChildSpawnerCommands,
    descriptor: &ActionDescriptor,
    ui_font: &UiFont,
    is_mobile: bool,
) {
    let node = if is_mobile {
        Node {
            width: Val::Percent(46.0),
            min_height: Val::Px(ACTION_BUTTON_TOUCH_TARGET),
            height: Val::Px(58.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            ..default()
        }
    } else {
        Node {
            width: Val::Px(ACTION_BUTTON_WIDTH),
            height: Val::Px(ACTION_BUTTON_HEIGHT),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            ..default()
        }
    };

    parent
        .spawn((
            Button,
            ActionButton {
                id: descriptor.id,
                intent: descriptor.intent,
            },
            node,
            BackgroundColor(BUTTON_NORMAL),
        ))
        .with_children(|button| {
            button
                .spawn((
                    Node {
                        width: Val::Px(34.0),
                        height: Val::Px(20.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(WALNUT),
                    BorderColor::all(GOLD),
                ))
                .with_children(|well| {
                    well.spawn((
                        Text::new(glyph_for(descriptor.pictogram_id)),
                        ui_font.text_font_bold(14.0),
                        TextColor(GOLD),
                        ActionGlyph,
                    ));
                });
            button.spawn((
                Text::new(descriptor.label),
                ui_font.text_font(15.0),
                TextColor(CREAM),
            ));
            button.spawn((
                Text::new(descriptor.cost.display_text()),
                ui_font.text_font(11.0),
                TextColor(CREAM),
                ActionCostOrReason,
            ));
        });
}

/// Placeholder ASCII glyph for `pictogram_id`, pending #122's real art keyed
/// off the same string contract ([`ActionDescriptor::pictogram_id`]). Falls
/// back to `"?"` for any id this match doesn't recognize, so a newly
/// registered descriptor (e.g. the extensibility test's eighth one) always
/// renders a button — this function is cosmetic-only and never needs an
/// edit for the palette itself to keep working.
fn glyph_for(pictogram_id: ActionId) -> &'static str {
    match pictogram_id {
        "quick-strike" => ">>",
        "heavy-strike" => "**",
        "block" => "[]",
        "rest" => "++",
        "step-forward" => "->",
        "step-back" => "<-",
        "leap-forward" => "^>",
        _ => "?",
    }
}

/// Query filter: enabled action buttons whose interaction changed this frame
/// (same shape as the menu's `ChangedEnabledButton`).
type ChangedEnabledButton = (
    Changed<Interaction>,
    With<Button>,
    With<ActionButton>,
    Without<DisabledButton>,
);

/// Emits the clicked button's descriptor intent as a [`PlayerActionEvent`] —
/// the same message the debug keyboard mapping writes. Disabled buttons are
/// filtered out entirely.
pub(super) fn handle_action_buttons(
    interactions: Query<(&Interaction, &ActionButton), ChangedEnabledButton>,
    mut actions: MessageWriter<PlayerActionEvent>,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            actions.write(PlayerActionEvent(button.intent));
        }
    }
}

/// Hover/pressed background feedback for enabled action buttons (the same
/// pattern as the main menu, scoped to the HUD's buttons).
pub(super) fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedEnabledButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

/// Query data for [`update_action_buttons`]: a button, its descriptor id,
/// whether it is currently disabled, and what it needs restyled.
type AvailabilityControlled = (
    Entity,
    &'static ActionButton,
    Has<DisabledButton>,
    &'static mut BackgroundColor,
    &'static Children,
);

/// Query data for the player fighter's stamina/attributes, filtered against
/// the enemy marker so it never aliases the enemy query below.
type PlayerStats<'w, 's> = Query<
    'w,
    's,
    (&'static Stamina, &'static Attributes),
    (With<PlayerFighter>, Without<EnemyFighter>),
>;

/// Greys out (and un-greys) action buttons to match each button's current
/// [`ActionDescriptor::enabled`], and swaps the cost-line text to the
/// descriptor's [`ActionDescriptor::disabled_reason`] while disabled (#189's
/// "expose their reason" acceptance criterion). Only touches buttons whose
/// enabled state actually flipped, so it does not fight the hover-feedback
/// system — the exact cadence the pre-#189 HUD already used for color alone.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_action_buttons(
    mut commands: Commands,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    extra: Res<ExtraDescriptors>,
    player: PlayerStats,
    enemy: Query<&Attributes, (With<EnemyFighter>, Without<PlayerFighter>)>,
    mut buttons: Query<AvailabilityControlled, With<Button>>,
    mut text_nodes: Query<(&mut TextColor, Option<&mut Text>, Has<ActionCostOrReason>)>,
) {
    let has_turn = turn.is_some();
    let (player_stamina, player_attributes) = player
        .single()
        .map(|(stamina, attrs)| (stamina.current, *attrs))
        .unwrap_or_default();
    let enemy_attributes = enemy.single().copied().unwrap_or_default();
    let presentation_busy = presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy);
    let ctx = DescriptorContext {
        turn: turn
            .as_deref()
            .copied()
            .unwrap_or_else(|| DescriptorContext::spawn_placeholder().turn),
        player_stamina,
        player_attributes,
        enemy_attributes,
        presentation_busy,
    };
    let mut descriptors = generate_action_descriptors(&ctx);
    descriptors.extend(extra.0.iter().cloned());

    for (entity, button, was_disabled, mut background, children) in &mut buttons {
        let Some(descriptor) = descriptors.iter().find(|d| d.id == button.id) else {
            continue;
        };
        // No `CombatTurn` yet (the fighters haven't spawned this frame):
        // matches the pre-#189 HUD's fallback of treating every action as
        // disabled rather than making a claim about state that isn't real
        // yet.
        let enabled = has_turn && descriptor.enabled;
        if enabled != was_disabled {
            // Already in the right state; leave hover feedback alone.
            continue;
        }
        let text_color = if enabled {
            commands.entity(entity).remove::<DisabledButton>();
            background.0 = BUTTON_NORMAL;
            CREAM
        } else {
            commands.entity(entity).insert(DisabledButton);
            background.0 = BUTTON_DISABLED;
            TEXT_DISABLED
        };
        for child in children.iter() {
            if let Ok((mut color, text, is_cost_or_reason)) = text_nodes.get_mut(child) {
                color.0 = text_color;
                if is_cost_or_reason && let Some(mut text) = text {
                    text.0 = if enabled {
                        descriptor.cost.display_text()
                    } else {
                        descriptor
                            .disabled_reason
                            .clone()
                            .unwrap_or_else(|| "Lupta nu a început încă.".to_string())
                    };
                }
            }
        }
    }
}

/// Re-flows the action buttons' size when [`crate::core::ViewportInfo`]
/// crosses the mobile breakpoint. Split out from `hud::apply_responsive_hud_layout`
/// (which still owns the panels/action-bar-root/log resizing) so this
/// module owns every `ActionButton`-shaped node in one place.
pub(super) fn apply_responsive_action_buttons(
    viewport: Res<crate::core::ViewportInfo>,
    mut buttons: Query<&mut Node, With<ActionButton>>,
) {
    if !viewport.is_changed() {
        return;
    }
    let is_mobile = viewport.is_mobile;
    for mut node in &mut buttons {
        if is_mobile {
            node.width = Val::Percent(46.0);
            node.min_height = Val::Px(ACTION_BUTTON_TOUCH_TARGET);
            node.height = Val::Px(58.0);
        } else {
            node.width = Val::Px(ACTION_BUTTON_WIDTH);
            node.min_height = Val::Auto;
            node.height = Val::Px(ACTION_BUTTON_HEIGHT);
        }
    }
}

/// The action bar's actual rendered padding after `panel_bundle` merges
/// `ACTION_BAR_PADDING` with the border inset (#120) — whichever is larger,
/// per side. Kept in sync with `merge_panel_padding`'s per-side rule so this
/// fit check reflects reality instead of the pre-merge constant.
#[cfg(test)]
fn desktop_action_strip_effective_padding() -> f32 {
    ACTION_BAR_PADDING.max(PANEL_BORDER_INSET)
}

#[cfg(test)]
fn desktop_action_strip_occupied_width() -> f32 {
    ACTION_BUTTON_WIDTH * ACTION_BUTTON_COUNT
        + ACTION_BAR_DESKTOP_GAP * (ACTION_BUTTON_COUNT - 1.0)
        + desktop_action_strip_effective_padding() * 2.0
}

#[cfg(test)]
fn desktop_action_strip_available_width() -> f32 {
    HUD_TARGET_WIDTH - ACTION_BAR_DESKTOP_INSET * 2.0
}

#[cfg(test)]
mod tests {
    use super::super::hud::HudScreen;
    use super::super::systems::{CombatPlugin, CombatRng, CombatSide};
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::Attributes as AttributesType;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MIN};
    use crate::combat::actions::{ActionCategory, ActionCost};
    use crate::core::{CorePlugin, GameState};
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use bevy::state::app::StatesPlugin;
    use rand::{RngExt as _, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use std::time::Duration;

    const PLAYER_ATTRIBUTES: AttributesType = AttributesType {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    fn strikes_rng(strikes: usize) -> ChaCha8Rng {
        'seed: for seed in 0..1_000_000u64 {
            let mut probe = ChaCha8Rng::seed_from_u64(seed);
            for _ in 0..strikes {
                if probe.random_range(0..100) >= HIT_PERCENT_MIN
                    || probe.random_range(0..100) < CRIT_PERCENT_CAP
                {
                    continue 'seed;
                }
            }
            return ChaCha8Rng::seed_from_u64(seed);
        }
        panic!("no seed under 1000000 lands {strikes} clean strikes");
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(10));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(CombatRng(strikes_rng(4)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter + first combat frame
        app
    }

    fn find_button(app: &mut App, id: ActionId) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &ActionButton), With<Button>>()
            .iter(app.world())
            .find(|(_, button)| button.id == id)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button {id} exists"))
    }

    fn find_button_by_action(app: &mut App, action: CombatAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &ActionButton), With<Button>>()
            .iter(app.world())
            .find(|(_, button)| button.intent == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button for {action:?} exists"))
    }

    fn press_button(app: &mut App, entity: Entity) {
        app.world_mut()
            .entity_mut(entity)
            .insert(Interaction::Pressed);
        app.update();
    }

    fn advance_presentation(app: &mut App) {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(super::super::systems::PRESENTATION_DELAY_SECONDS + 0.1),
        ));
        app.update();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
    }

    fn press_button_and_wait(app: &mut App, action: CombatAction) {
        let entity = find_button_by_action(app, action);
        press_button(app, entity);
        advance_presentation(app);
        advance_presentation(app);
    }

    fn turn(app: &App) -> CombatTurn {
        *app.world().resource::<CombatTurn>()
    }

    fn pools<M: Component>(app: &mut App) -> (i32, i32) {
        let (health, stamina) = app
            .world_mut()
            .query_filtered::<(&crate::character::Health, &Stamina), With<M>>()
            .single(app.world())
            .expect("fighter exists");
        (health.current, stamina.current)
    }

    fn player_pools(app: &mut App) -> (i32, i32) {
        pools::<PlayerFighter>(app)
    }

    fn enemy_pools(app: &mut App) -> (i32, i32) {
        pools::<EnemyFighter>(app)
    }

    fn drain_enemy_stamina(app: &mut App) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Stamina, With<EnemyFighter>>();
        query
            .single_mut(app.world_mut())
            .expect("enemy fighter exists")
            .current = 0;
    }

    fn set_player_stamina(app: &mut App, value: i32) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Stamina, With<PlayerFighter>>();
        query
            .single_mut(app.world_mut())
            .expect("player fighter exists")
            .current = value;
    }

    // --- pure descriptor generation used by the palette ---

    #[test]
    fn spawn_placeholder_produces_all_seven_actions_with_no_disabled_reason() {
        let descriptors = generate_action_descriptors(&DescriptorContext::spawn_placeholder());
        assert_eq!(descriptors.len(), 7);
    }

    // --- headless screen behavior (moved from the pre-#189 hud.rs) ---

    #[test]
    fn entering_fight_spawns_all_seven_action_buttons() {
        let mut app = test_app();
        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            buttons, 7,
            "four combat buttons plus three movement buttons"
        );
    }

    #[test]
    fn action_tiles_are_icon_led_and_fit_the_desktop_strip() {
        let mut app = test_app();

        let glyphs = app
            .world_mut()
            .query::<&ActionGlyph>()
            .iter(app.world())
            .count();
        assert_eq!(glyphs, 7, "every action button has a glyph marker");

        let mut buttons = app
            .world_mut()
            .query_filtered::<&Node, With<ActionButton>>();
        for node in buttons.iter(app.world()) {
            assert_eq!(node.width, Val::Px(ACTION_BUTTON_WIDTH));
            assert_eq!(node.height, Val::Px(ACTION_BUTTON_HEIGHT));
        }
        assert!(
            desktop_action_strip_occupied_width() <= desktop_action_strip_available_width(),
            "desktop action strip must fit the 800px target viewport"
        );
    }

    #[test]
    fn narrowing_the_viewport_grows_the_action_buttons() {
        let mut app = test_app();
        app.update();

        app.world_mut()
            .resource_mut::<crate::core::ViewportInfo>()
            .set_if_neq(crate::core::ViewportInfo {
                width: 375.0,
                height: 812.0,
                is_mobile: true,
            });
        app.update();

        let mut buttons = app
            .world_mut()
            .query_filtered::<&Node, With<ActionButton>>();
        for node in buttons.iter(app.world()) {
            let Val::Px(min_height) = node.min_height else {
                panic!("expected a pixel min height on mobile");
            };
            assert!(
                min_height >= crate::theme::ACTION_BUTTON_TOUCH_TARGET,
                "action button min height {min_height} below the touch target"
            );
        }
    }

    #[test]
    fn pressing_the_quick_strike_button_plays_the_action() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        press_button_and_wait(&mut app, CombatAction::QuickStrike);
        assert_eq!(enemy_pools(&mut app), (64, 20));
        assert_eq!(player_pools(&mut app), (90, 45));
        assert_eq!(turn(&app).side, CombatSide::Player);
    }

    #[test]
    fn buttons_disable_exactly_when_the_action_is_unavailable_and_show_a_reason() {
        let mut app = test_app();
        set_player_stamina(&mut app, 10);
        app.update();

        let heavy = find_button(&mut app, "heavy-strike");
        assert!(
            app.world().entity(heavy).contains::<DisabledButton>(),
            "heavy strike greys out below its 15 cost"
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(heavy).map(|b| b.0),
            Some(BUTTON_DISABLED)
        );

        for affordable in ["quick-strike", "block", "rest", "step-back"] {
            let button = find_button(&mut app, affordable);
            assert!(
                !app.world().entity(button).contains::<DisabledButton>(),
                "{affordable} stays enabled at 10 stamina"
            );
        }
        for unavailable in ["step-forward", "leap-forward"] {
            let button = find_button(&mut app, unavailable);
            assert!(
                app.world().entity(button).contains::<DisabledButton>(),
                "{unavailable} greys out while already close"
            );
        }

        // A press on the disabled button is inert: no action resolves.
        let before = (player_pools(&mut app), enemy_pools(&mut app));
        press_button(&mut app, heavy);
        assert_eq!((player_pools(&mut app), enemy_pools(&mut app)), before);
        assert_eq!(turn(&app).side, CombatSide::Player, "turn did not pass");

        // The subtitle line now shows a specific Romanian reason instead of
        // the cost.
        let reason_text = find_cost_or_reason_text(&mut app, heavy);
        assert_eq!(reason_text, "Stamina insuficientă (nevoie 15).");
    }

    fn find_cost_or_reason_text(app: &mut App, button: Entity) -> String {
        let children = app
            .world()
            .get::<Children>(button)
            .expect("button has children")
            .to_vec();
        for child in children {
            if app.world().get::<ActionCostOrReason>(child).is_some() {
                return app
                    .world()
                    .get::<Text>(child)
                    .expect("cost/reason node has Text")
                    .0
                    .clone();
            }
        }
        panic!("no ActionCostOrReason child found");
    }

    #[test]
    fn a_fight_is_playable_start_to_finish_with_the_mouse_only() {
        let mut app = test_app();
        for _ in 0..200 {
            if turn(&app).over {
                break;
            }
            drain_enemy_stamina(&mut app);
            let action = if player_pools(&mut app).1 >= CombatAction::QuickStrike.stamina_cost() {
                CombatAction::QuickStrike
            } else {
                CombatAction::Rest
            };
            press_button_and_wait(&mut app, action);
        }
        assert!(turn(&app).over, "duel ends");
        assert_eq!(enemy_pools(&mut app).0, 0, "enemy is defeated");
        assert!(player_pools(&mut app).0 > 0, "player survives");
        for id in [
            "quick-strike",
            "heavy-strike",
            "block",
            "rest",
            "step-forward",
            "step-back",
            "leap-forward",
        ] {
            let button = find_button(&mut app, id);
            assert!(
                app.world().entity(button).contains::<DisabledButton>(),
                "{id} greys out once the duel is over"
            );
        }
    }

    #[test]
    fn leaving_the_fight_despawns_the_action_buttons() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        let buttons = app
            .world_mut()
            .query_filtered::<(), With<ActionButton>>()
            .iter(app.world())
            .count();
        assert_eq!(buttons, 0);
        let hud = app
            .world_mut()
            .query_filtered::<(), With<HudScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(hud, 0);
    }

    // --- extensibility seam (#189 acceptance criterion) ---

    #[test]
    fn a_test_registered_eighth_descriptor_renders_and_emits_with_no_layout_edits() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(10));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(CombatRng(strikes_rng(4)));
        // Register the eighth descriptor *before* the fight screen (and thus
        // the HUD) spawns, exactly like a real registration would exist
        // ahead of time -- `ExtraDescriptors` is read by both
        // `spawn_action_bar` and `update_action_buttons`, never special-cased.
        app.insert_resource(ExtraDescriptors(vec![ActionDescriptor {
            id: "test-extra-action",
            category: ActionCategory::Special,
            label: "Acțiune de test",
            pictogram_id: "test-extra-action",
            cost: ActionCost::None,
            hit_chance: None,
            position_legal: true,
            enabled: true,
            disabled_reason: None,
            intent: CombatAction::Rest,
        }]));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();

        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            buttons, 8,
            "seven real descriptors plus the test-registered eighth"
        );

        let extra_button = find_button(&mut app, "test-extra-action");
        assert!(
            !app.world()
                .entity(extra_button)
                .contains::<DisabledButton>(),
            "the extra descriptor is enabled and must render as such"
        );

        // Pressing it emits the existing PlayerActionEvent(Rest) command --
        // proof that a registered descriptor emits through the same path as
        // the seven real ones, with zero edits to this module's layout code.
        drain_enemy_stamina(&mut app);
        press_button(&mut app, extra_button);
        advance_presentation(&mut app);
        advance_presentation(&mut app);
        assert_eq!(
            player_pools(&mut app).1,
            50,
            "Rest at full stamina restores nothing but still resolves and passes the turn"
        );
        assert_eq!(
            turn(&app).side,
            CombatSide::Player,
            "the turn came back after the enemy's reply"
        );
    }

    #[test]
    fn without_a_test_registration_the_palette_stays_at_seven_buttons() {
        let mut app = test_app();
        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(buttons, 7, "ExtraDescriptors defaults to empty");
    }
}
