//! Combat action palette (#189/#199, children of #143): renders the desktop
//! action bar and the phone category-disclosure palette entirely from
//! [`ActionDescriptor`]s. `hud::spawn_hud` delegates the action-bar subtree
//! to [`spawn_action_bar`] below instead of hard-coding buttons; every
//! subsequent frame, [`update_action_buttons`] re-derives the same
//! descriptors from live duel state and reconciles the already-spawned
//! buttons against them.
//!
//! ## Desktop (#189)
//!
//! A single, non-wrapping row of one button per descriptor — untouched by
//! #199.
//!
//! ## Phone (#199)
//!
//! At the mobile breakpoint the bar instead shows at most four large
//! [`CategoryButton`]s — one per non-empty [`super::actions::ActionCategory`],
//! grouped via [`super::actions::group_by_category`] — plus a second row
//! that stays empty while closed. Tapping a category opens it:
//! [`PhonePaletteState`] tracks which one (if any), [`handle_category_buttons`]
//! toggles it on click, and [`sync_phone_open_category`] rebuilds the second
//! row's children — real [`ActionButton`]s, wired through the exact same
//! [`handle_action_buttons`]/[`update_action_buttons`] systems desktop's
//! buttons use — to match. Tapping the open category again (or a different
//! one) closes/switches it; never more than one category's actions are
//! visible at once, and opening/closing never touches duel state (fighters,
//! [`super::systems::CombatTurn`], stamina/health) — only this small HUD
//! subtree. Crossing the mobile breakpoint at runtime
//! ([`rebuild_action_bar_on_breakpoint_change`]) rebuilds the whole bar for
//! the new layout rather than resizing buttons in place, since the two
//! layouts are structurally different (a flat row vs. category disclosure);
//! desktop's own buttons are never touched by that rebuild path either way,
//! since their sizing is fixed, not viewport-driven.
//!
//! See the `extensibility_seam` test module below for proof the button *set*
//! always comes from [`super::actions::generate_action_descriptors`] plus
//! [`super::actions::ExtraDescriptors`], never a fixed list of match arms —
//! and the `phone_palette` test module for proof category membership always
//! comes from [`ActionDescriptor::category`], including for a
//! test-registered descriptor.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::character::{Attributes, EnemyFighter, PlayerFighter, Stamina};
use crate::core::{UiFont, ViewportInfo};
use crate::menu::DisabledButton;
use crate::theme::{
    ACTION_BUTTON_TOUCH_TARGET, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED,
    CREAM, GOLD, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
use crate::ui_widgets::focus::{Focusable, TabGroup, TabIndex, redirect_focus_if_inside};

use super::actions::{
    ActionCategory, ActionDescriptor, ActionId, DescriptorContext, ExtraDescriptors,
    category_label, generate_action_descriptors, group_by_category,
};
use super::engine::CombatAction;
use super::hud::{ActionBarRoot, HudScreen};
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
/// Conservative side margin the desktop strip's fit check reserves against
/// `HUD_TARGET_WIDTH` (see `desktop_action_strip_available_width`). The
/// *rendered* desktop bar spans the full stage width (`left`/`right` 0):
/// pre-#199, `hud::apply_responsive_hud_layout` always overwrote the
/// spawn-time 10px insets to 0 before the first layout pass, so 0 is the
/// value every accepted desktop baseline actually shows — #199 spawns with
/// it directly (byte-identical desktop) instead of patching after the fact.
#[cfg(test)]
const ACTION_BAR_DESKTOP_INSET: f32 = 10.0;
#[cfg(test)]
const HUD_TARGET_WIDTH: f32 = 800.0;
#[cfg(test)]
const ACTION_BUTTON_COUNT: f32 = 7.0;

/// Row height for every phone control — category buttons and open-category
/// action buttons alike (#199) — comfortably above the 44px CSS touch-target
/// floor the issue requires (also above [`ACTION_BUTTON_TOUCH_TARGET`], the
/// pre-#199 mobile minimum, so this is never a shrink).
const PHONE_TARGET_HEIGHT: f32 = 56.0;
/// Gap between phone controls in the same row, and between the category row
/// and the (when open) action row above it.
const PHONE_ROW_GAP: f32 = 8.0;

/// The combat action a HUD button submits when clicked, plus the stable
/// descriptor id it was built from — id-keyed (not action-keyed) lookup so
/// two descriptors can in principle share the same [`CombatAction`] intent
/// (the extensibility test's eighth descriptor does exactly this) without
/// becoming ambiguous.
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's `fight-palette-desktop`
/// and `fight-palette-phone` browser scenarios (#189/#199) read this marker
/// (plus each button's real `ComputedNode`/`UiGlobalTransform`) to publish an
/// exact geometric "every button rendered inside the letterboxed stage rect"
/// fact, computed once in native Bevy space rather than duplicated pixel-math
/// on the browser-harness side.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionButton {
    pub id: ActionId,
    pub intent: CombatAction,
}

/// One phone category control (#199) — see [`spawn_category_button`].
/// `pub(crate)` for the same reason [`ActionButton`] is: the `review`
/// feature's `fight-palette-phone` browser scenario reads this marker (plus
/// each button's real geometry) to publish phone palette telemetry.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CategoryButton {
    pub category: ActionCategory,
}

/// Which phone category (if any) is currently open (#199). Reset to closed
/// every time a fight starts (`combat::systems::setup_combat`) and whenever
/// the mobile breakpoint is crossed at runtime
/// ([`rebuild_action_bar_on_breakpoint_change`]), so a category left open
/// never survives past the UI subtree — or the fight — it belonged to.
/// `pub(crate)` so the `review` feature can read which category (if any) is
/// currently open for its `fight-palette-phone` telemetry.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PhonePaletteState {
    pub open: Option<ActionCategory>,
}

/// Marker for the phone bar's always-visible category row (#199): up to four
/// [`CategoryButton`]s, spawned once per fight (category membership is
/// stable for the lifetime of a fight).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct PhoneCategoryRow;

/// Marker for the phone bar's action row (#199): empty while
/// [`PhonePaletteState::open`] is `None`, populated with the open category's
/// real [`ActionButton`]s by [`sync_phone_open_category`] otherwise.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PhoneActionsRow;

/// Small glyph at the head of an action tile; stable so tests can confirm
/// buttons are icon-led without depending on screenshots. Purely cosmetic —
/// no payload — since #122's real pictogram art will key off
/// [`ActionDescriptor::pictogram_id`] directly rather than this marker.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionGlyph;

/// Marker for the text node that shows an action's cost line when enabled
/// and its disabled reason when it isn't (#189's "expose their reason"
/// acceptance criterion) — the same text slot, never a new UI element.
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's `fight-palette-
/// accessible` browser scenario (#213) reads this marker (plus the current
/// [`bevy::input_focus::InputFocus`]) to publish the focused control's shown
/// reason text, so the scenario can assert a real, rendered Romanian
/// sentence rather than a screenshot pixel probe.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionCostOrReason;

/// The bottom action bar: desktop's single wrapping row (#189, unchanged) or
/// phone's category-disclosure stack (#199, see [`spawn_phone_action_bar`]).
/// Both iterate [`generate_action_descriptors`] plus [`ExtraDescriptors`] —
/// never a hard-coded button list — so a later registered action renders
/// here with no edits to this function.
///
/// Spawned with [`DescriptorContext::spawn_placeholder`] (see its docs):
/// `CombatTurn` does not exist yet at this point in the `OnEnter(Fight)`
/// schedule, so every button spawns showing its cost line, uncolored as
/// disabled; [`update_action_buttons`] corrects colors and text against real
/// state on the very next frame, exactly like the pre-#189 HUD did for
/// button color alone. The phone bar's action row is spawned empty
/// regardless (closed by default), so this placeholder-vs-real distinction
/// only matters for desktop's/the category row's *cosmetic* fields (label,
/// cost text, glyph) — never an enabled/disabled claim.
pub(super) fn spawn_action_bar(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
    extra: &ExtraDescriptors,
) {
    if is_mobile {
        spawn_phone_action_bar(parent, ui_font, extra);
        return;
    }

    let node = Node {
        position_type: PositionType::Absolute,
        bottom: Val::Px(12.0),
        // 0, not `ACTION_BAR_DESKTOP_INSET`: the rendered pre-#199 value --
        // see that constant's doc comment for why the insets never actually
        // painted.
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::NoWrap,
        justify_content: JustifyContent::Center,
        row_gap: Val::Px(0.0),
        column_gap: Val::Px(ACTION_BAR_DESKTOP_GAP),
        padding: UiRect::all(Val::Px(ACTION_BAR_PADDING)),
        ..default()
    };

    let mut descriptors = generate_action_descriptors(&DescriptorContext::spawn_placeholder());
    descriptors.extend(extra.0.iter().cloned());

    parent
        .spawn((
            panel_bundle(panel_texture, node),
            BackgroundColor(PANEL_LINEN),
            ActionBarRoot,
            // #213: one shared focus region for the whole bar — see
            // `crate::ui_widgets::focus`'s registration API.
            TabGroup::new(0),
        ))
        .with_children(|bar| {
            for descriptor in &descriptors {
                spawn_action_button(bar, descriptor, ui_font);
            }
        });
}

/// #199's phone bar: a vertical stack of [`PhoneActionsRow`] (top, empty
/// while closed) over [`PhoneCategoryRow`] (bottom, up to four
/// always-visible category controls) — the container is bottom-anchored, so
/// it grows *upward* as the action row populates, and the category row's
/// own position never moves. Category membership comes from
/// [`group_by_category`] on the same placeholder descriptor set desktop's
/// initial spawn uses (see [`DescriptorContext::spawn_placeholder`]'s docs)
/// — real duel state does not exist yet this early in the
/// `OnEnter(GameState::Fight)` schedule, but category membership does not
/// depend on it.
///
/// Deliberately *not* `panel_bundle`-decorated like desktop's bar: the
/// embroidered panel's [`crate::theme::PANEL_BORDER_INSET`] floors padding
/// at 24px per side, and inside the 4:3 stage a 390px-wide phone letterboxes
/// to (390 × 292.5px), that extra 32px of vertical padding would push the
/// open two-row palette up into the fighter status panels — exactly what
/// #199 forbids ("does not cover required fighter/status information").
/// A plain [`PANEL_LINEN`] backdrop with slim padding keeps both 56px rows
/// clear of the nameplates without shrinking any touch target.
fn spawn_phone_action_bar(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    extra: &ExtraDescriptors,
) {
    let node = Node {
        position_type: PositionType::Absolute,
        bottom: Val::Px(8.0),
        left: Val::Px(8.0),
        right: Val::Px(8.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(PHONE_ROW_GAP),
        padding: UiRect::all(Val::Px(ACTION_BAR_PADDING)),
        ..default()
    };

    let mut descriptors = generate_action_descriptors(&DescriptorContext::spawn_placeholder());
    descriptors.extend(extra.0.iter().cloned());
    let groups = group_by_category(&descriptors);

    parent
        .spawn((
            node,
            BackgroundColor(PANEL_LINEN),
            ActionBarRoot,
            // #213: one shared focus region for both rows — see
            // `crate::ui_widgets::focus`'s registration API. The action row
            // is spawned first (top), so it tabs before the category row
            // (bottom) whenever it is populated -- both rows read top to
            // bottom, matching this tree order.
            TabGroup::new(0),
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(PHONE_ROW_GAP),
                    ..default()
                },
                PhoneActionsRow,
            ));
            bar.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(PHONE_ROW_GAP),
                    ..default()
                },
                PhoneCategoryRow,
            ))
            .with_children(|row| {
                for (category, _members) in &groups {
                    spawn_category_button(row, *category, ui_font);
                }
            });
        });
}

/// One phone category control (#199): a large, equal-width tile (up to four
/// share the row) at least [`PHONE_TARGET_HEIGHT`] tall — comfortably above
/// the 44px CSS touch-target floor. [`update_category_button_backgrounds`]
/// keeps its background in sync with hover/press feedback and whether
/// [`PhonePaletteState`] currently has it open.
fn spawn_category_button(
    parent: &mut ChildSpawnerCommands,
    category: ActionCategory,
    ui_font: &UiFont,
) {
    parent
        .spawn((
            Button,
            CategoryButton { category },
            Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                min_width: Val::Px(ACTION_BUTTON_TOUCH_TARGET),
                min_height: Val::Px(PHONE_TARGET_HEIGHT),
                height: Val::Px(PHONE_TARGET_HEIGHT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_NORMAL),
            // #213: category buttons are always focusable — never disabled.
            Focusable,
            TabIndex(0),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(category_label(category)),
                ui_font.text_font_bold(16.0),
                TextColor(CREAM),
            ));
        });
}

/// The small carved-wood glyph well shared by every action tile — factored
/// out of [`spawn_action_button`]/[`spawn_phone_action_button`] so desktop
/// and phone action buttons can never drift on how they render an action's
/// icon.
fn spawn_glyph_well(parent: &mut ChildSpawnerCommands, pictogram_id: ActionId, ui_font: &UiFont) {
    parent
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
                Text::new(glyph_for(pictogram_id)),
                ui_font.text_font_bold(14.0),
                TextColor(GOLD),
                ActionGlyph,
            ));
        });
}

/// One desktop action button: the Romanian label over its cost/disabled-
/// reason line, in a fixed-size tile. Unchanged by #199 — phone action
/// buttons are [`spawn_phone_action_button`] instead, a structurally
/// different (row-flexed, real-enabled-state-at-spawn) tile.
fn spawn_action_button(
    parent: &mut ChildSpawnerCommands,
    descriptor: &ActionDescriptor,
    ui_font: &UiFont,
) {
    let node = Node {
        width: Val::Px(ACTION_BUTTON_WIDTH),
        height: Val::Px(ACTION_BUTTON_HEIGHT),
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        row_gap: Val::Px(2.0),
        ..default()
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
            // #213: disabled actions stay focusable so their reason is
            // reachable — see `crate::ui_widgets::focus`'s registration API.
            Focusable,
            TabIndex(0),
        ))
        .with_children(|button| {
            spawn_glyph_well(button, descriptor.pictogram_id, ui_font);
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

/// One phone action-row button (#199): like desktop's [`spawn_action_button`]
/// but row-flexed (equal width, sharing [`PhoneActionsRow`] with up to two
/// siblings so the row never wraps) and spawned with the *real* current
/// enabled/disabled state — unlike the bar's initial placeholder-based
/// spawn, a category can only open once the fight (and `CombatTurn`) already
/// exists, so there is real state to render immediately instead of waiting a
/// frame for [`update_action_buttons`] to correct it.
fn spawn_phone_action_button(
    parent: &mut ChildSpawnerCommands,
    descriptor: &ActionDescriptor,
    ui_font: &UiFont,
    enabled: bool,
) {
    let (background, text_color) = if enabled {
        (BUTTON_NORMAL, CREAM)
    } else {
        (BUTTON_DISABLED, TEXT_DISABLED)
    };
    let cost_or_reason = if enabled {
        descriptor.cost.display_text()
    } else {
        descriptor
            .disabled_reason
            .clone()
            .unwrap_or_else(|| "Lupta nu a început încă.".to_string())
    };

    let mut button = parent.spawn((
        Button,
        ActionButton {
            id: descriptor.id,
            intent: descriptor.intent,
        },
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            min_width: Val::Px(ACTION_BUTTON_TOUCH_TARGET),
            min_height: Val::Px(PHONE_TARGET_HEIGHT),
            height: Val::Px(PHONE_TARGET_HEIGHT),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            ..default()
        },
        BackgroundColor(background),
        // #213: focusable regardless of `enabled` — see this bar's
        // registration API doc in `crate::ui_widgets::focus`.
        Focusable,
        TabIndex(0),
    ));
    if !enabled {
        button.insert(DisabledButton);
    }
    button.with_children(|button| {
        spawn_glyph_well(button, descriptor.pictogram_id, ui_font);
        button.spawn((
            Text::new(descriptor.label),
            ui_font.text_font(15.0),
            TextColor(text_color),
        ));
        button.spawn((
            Text::new(cost_or_reason),
            ui_font.text_font(11.0),
            TextColor(text_color),
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
/// filtered out entirely. Applies identically to desktop and phone action
/// buttons — both carry the same [`ActionButton`] component, so a selected
/// phone action emits the exact same command a desktop click would (#199).
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

/// Query filter: category buttons whose interaction changed this frame (same
/// shape as [`ChangedEnabledButton`]; categories are never disabled).
type ChangedCategoryButton = (Changed<Interaction>, With<Button>);

/// Toggles [`PhonePaletteState::open`] when a category button is pressed
/// (#199): pressing the already-open category closes it, pressing a
/// different one switches directly to it. Only ever touches this resource —
/// never duel state — so opening/closing never causes a re-spawn side
/// effect on the fighters or the turn.
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's
/// `poll_review_commands` orders itself `.before(...)` this system so a
/// same-frame `pressActionCategory`'s `Interaction::Pressed` write is
/// observed here before `bevy_ui`'s focus system can reset it next
/// `PreUpdate` — the same reasoning `FlowIntentEmission` documents for
/// `pressButton`.
pub(crate) fn handle_category_buttons(
    interactions: Query<(&Interaction, &CategoryButton), ChangedCategoryButton>,
    mut state: ResMut<PhonePaletteState>,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            state.open = if state.open == Some(button.category) {
                None
            } else {
                Some(button.category)
            };
        }
    }
}

/// Hover/pressed feedback for category buttons, plus a persistent highlight
/// on whichever category [`PhonePaletteState`] currently has open. Cheap
/// enough to run unconditionally every frame — at most four entities.
pub(super) fn update_category_button_backgrounds(
    state: Res<PhonePaletteState>,
    mut buttons: Query<(&Interaction, &CategoryButton, &mut BackgroundColor)>,
) {
    for (interaction, button, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None if state.open == Some(button.category) => BUTTON_PRESSED,
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

/// Type alias for the enemy attributes query both [`update_action_buttons`]
/// and [`sync_phone_open_category`] read, kept in one place so the two
/// systems' signatures can never drift apart.
type EnemyStats<'w, 's> =
    Query<'w, 's, &'static Attributes, (With<EnemyFighter>, Without<PlayerFighter>)>;

/// Builds the live descriptor list both [`update_action_buttons`] (desktop
/// and phone reconciliation alike) and [`sync_phone_open_category`] (the
/// phone action row's population) need — factored out so the two can never
/// derive "what can the player do right now" differently.
fn live_descriptors(
    turn: Option<&CombatTurn>,
    presentation_busy: bool,
    player: &PlayerStats,
    enemy: &EnemyStats,
    extra: &ExtraDescriptors,
) -> Vec<ActionDescriptor> {
    let (player_stamina, player_attributes) = player
        .single()
        .map(|(stamina, attrs)| (stamina.current, *attrs))
        .unwrap_or_default();
    let enemy_attributes = enemy.single().copied().unwrap_or_default();
    let ctx = DescriptorContext {
        turn: turn
            .copied()
            .unwrap_or_else(|| DescriptorContext::spawn_placeholder().turn),
        player_stamina,
        player_attributes,
        enemy_attributes,
        presentation_busy,
    };
    let mut descriptors = generate_action_descriptors(&ctx);
    descriptors.extend(extra.0.iter().cloned());
    descriptors
}

/// Greys out (and un-greys) action buttons to match each button's current
/// [`ActionDescriptor::enabled`], and swaps the cost-line text to the
/// descriptor's [`ActionDescriptor::disabled_reason`] while disabled (#189's
/// "expose their reason" acceptance criterion). Only touches buttons whose
/// enabled state actually flipped, so it does not fight the hover-feedback
/// system — the exact cadence the pre-#189 HUD already used for color alone.
/// Applies identically to desktop's seven buttons and phone's (0–3) open
/// action-row buttons — both carry the same [`ActionButton`] component.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_action_buttons(
    mut commands: Commands,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    extra: Res<ExtraDescriptors>,
    player: PlayerStats,
    enemy: EnemyStats,
    mut buttons: Query<AvailabilityControlled, With<Button>>,
    mut text_nodes: Query<(&mut TextColor, Option<&mut Text>, Has<ActionCostOrReason>)>,
) {
    let has_turn = turn.is_some();
    let presentation_busy = presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy);
    let descriptors = live_descriptors(turn.as_deref(), presentation_busy, &player, &enemy, &extra);

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

/// Rebuilds the phone action row's children (#199) whenever
/// [`PhonePaletteState`] changes: despawns whatever was there, then — if a
/// category is now open — spawns real [`ActionButton`]s for exactly that
/// category's registered descriptors (via [`live_descriptors`] +
/// [`super::actions::group_by_category`]'s membership rule), each carrying
/// its true current enabled/disabled state. Closing (or switching away from)
/// a category despawns its buttons without touching anything else — no
/// duel-state side effect.
///
/// #213: if focus was on one of the about-to-despawn action buttons, it is
/// redirected — via [`redirect_focus_if_inside`] — to the category button of
/// whichever category was open *before* this change (`previously_open`, a
/// per-system [`Local`] snapshot taken on the previous changed frame). That
/// button is never despawned by this system (only the action row is), so it
/// is always a safe, still-alive neighbor: closing a category moves focus to
/// its own control, and switching to a different category does too (the
/// just-closed category's button, not the newly opened one) — a player who
/// tabs away from what they were looking at lands back on the exact control
/// that made it disappear.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_phone_open_category(
    mut commands: Commands,
    state: Res<PhonePaletteState>,
    ui_font: Res<UiFont>,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    extra: Res<ExtraDescriptors>,
    player: PlayerStats,
    enemy: EnemyStats,
    mut input_focus: ResMut<InputFocus>,
    categories: Query<(Entity, &CategoryButton)>,
    mut previously_open: Local<Option<ActionCategory>>,
    row: Query<(Entity, Option<&Children>), With<PhoneActionsRow>>,
) {
    if !state.is_changed() {
        return;
    }
    let closing = *previously_open;
    *previously_open = state.open;

    let Ok((row_entity, children)) = row.single() else {
        return;
    };
    if let Some(children) = children {
        let fallback = closing.and_then(|category| {
            categories
                .iter()
                .find(|(_, button)| button.category == category)
                .map(|(entity, _)| entity)
        });
        redirect_focus_if_inside(&mut input_focus, children.iter(), fallback);
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    let Some(open_category) = state.open else {
        return;
    };

    let has_turn = turn.is_some();
    let presentation_busy = presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy);
    let descriptors = live_descriptors(turn.as_deref(), presentation_busy, &player, &enemy, &extra);
    let members: Vec<ActionDescriptor> = descriptors
        .into_iter()
        .filter(|d| d.category == open_category)
        .collect();

    commands.entity(row_entity).with_children(|row| {
        for descriptor in &members {
            let enabled = has_turn && descriptor.enabled;
            spawn_phone_action_button(row, descriptor, &ui_font, enabled);
        }
    });
}

/// Rebuilds the whole action bar when [`ViewportInfo`] crosses the mobile
/// breakpoint (#199): desktop's flat row and phone's category disclosure are
/// structurally different layouts, not just different button sizes — unlike
/// the fighter panels/log panel (`hud::apply_responsive_hud_layout`, which
/// still resize those in place), the action bar's subtree is despawned and
/// respawned fresh via [`spawn_action_bar`] rather than patched. This never
/// touches duel state (fighters, [`CombatTurn`], stamina/health) — only the
/// small HUD subtree under [`ActionBarRoot`] — and resets
/// [`PhonePaletteState`] to closed so a category left open before the
/// crossing never survives it.
///
/// #213: also clears [`InputFocus`] on an actual rebuild. Unlike the phone
/// palette's own category open/close (a documented safe neighbor always
/// exists — see [`sync_phone_open_category`]), a breakpoint crossing
/// replaces the *entire* layout (seven flat buttons versus category
/// disclosure), so there is no single control on the new layout that is the
/// "same" one focus was on; clearing is the documented safe fallback here,
/// and the next Tab press lands on the new layout's first control (the same
/// behavior [`bevy::input_focus::tab_navigation::TabNavigation::navigate`]
/// already gives an unset focus).
#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild_action_bar_on_breakpoint_change(
    mut commands: Commands,
    viewport: Res<ViewportInfo>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    extra: Res<ExtraDescriptors>,
    mut phone_state: ResMut<PhonePaletteState>,
    mut input_focus: ResMut<InputFocus>,
    hud_root: Query<Entity, With<HudScreen>>,
    action_bar: Query<Entity, With<ActionBarRoot>>,
    mut last_is_mobile: Local<Option<bool>>,
) {
    let Ok(hud_root) = hud_root.single() else {
        // No HUD yet (outside the fight): forget the last-seen breakpoint so
        // the next fight's first frame is treated as a fresh baseline
        // instead of comparing against a stale value from a previous fight.
        *last_is_mobile = None;
        return;
    };
    match *last_is_mobile {
        None => {
            // First observation since this fight's HUD spawned: `spawn_hud`
            // already built the bar for the correct breakpoint, so just
            // record the baseline instead of rebuilding redundantly.
            *last_is_mobile = Some(viewport.is_mobile);
            return;
        }
        Some(previous) if previous == viewport.is_mobile => return,
        Some(_) => {}
    }
    *last_is_mobile = Some(viewport.is_mobile);
    if let Ok(old_bar) = action_bar.single() {
        commands.entity(old_bar).despawn();
    }
    *phone_state = PhonePaletteState::default();
    input_focus.clear();
    commands.entity(hud_root).with_children(|parent| {
        spawn_action_bar(parent, &ui_font, &panel_texture, viewport.is_mobile, &extra);
    });
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

    /// Like [`test_app`], but the mobile [`ViewportInfo`] is in place
    /// *before* the fight's `OnEnter` schedule runs, so `spawn_hud` (and
    /// therefore `spawn_action_bar`) observes it mobile from the very first
    /// spawn instead of needing a runtime breakpoint crossing.
    fn mobile_test_app() -> App {
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
        app.insert_resource(ViewportInfo {
            width: 390.0,
            height: 844.0,
            is_mobile: true,
        });
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

    fn action_button_ids(app: &mut App) -> Vec<ActionId> {
        let mut ids: Vec<ActionId> = app
            .world_mut()
            .query::<&ActionButton>()
            .iter(app.world())
            .map(|button| button.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn find_category_button(app: &mut App, category: ActionCategory) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &CategoryButton), With<Button>>()
            .iter(app.world())
            .find(|(_, button)| button.category == category)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("category button {category:?} exists"))
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

    // --- phone category disclosure (#199) ---

    mod phone_palette {
        use super::*;

        fn assert_meets_touch_target(node: &Node, label: &str) {
            let Val::Px(min_height) = node.min_height else {
                panic!(
                    "{label}: expected a pixel min height, got {:?}",
                    node.min_height
                );
            };
            assert!(
                min_height >= 44.0,
                "{label}: min height {min_height} below the 44px CSS touch-target floor"
            );
            let Val::Px(min_width) = node.min_width else {
                panic!(
                    "{label}: expected a pixel min width, got {:?}",
                    node.min_width
                );
            };
            assert!(
                min_width >= 44.0,
                "{label}: min width {min_width} below the 44px CSS touch-target floor"
            );
        }

        #[test]
        fn phone_layout_shows_at_most_four_category_buttons_meeting_the_touch_target() {
            let mut app = mobile_test_app();

            let categories: Vec<Entity> = app
                .world_mut()
                .query_filtered::<Entity, With<CategoryButton>>()
                .iter(app.world())
                .collect();
            assert!(
                !categories.is_empty() && categories.len() <= 4,
                "expected 1..=4 category controls, got {}",
                categories.len()
            );
            assert_eq!(
                categories.len(),
                4,
                "the seven real actions span exactly four categories today"
            );

            for entity in categories {
                let node = app.world().get::<Node>(entity).expect("category node");
                assert_meets_touch_target(node, "category button");
            }
        }

        #[test]
        fn phone_layout_starts_closed_with_no_action_buttons() {
            let mut app = mobile_test_app();
            let buttons = app
                .world_mut()
                .query_filtered::<(), With<ActionButton>>()
                .iter(app.world())
                .count();
            assert_eq!(buttons, 0, "closed by default: no category is open yet");
        }

        #[test]
        fn tapping_a_category_opens_only_its_registered_actions() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);

            let ids = action_button_ids(&mut app);
            assert_eq!(
                ids,
                vec!["heavy-strike", "quick-strike"],
                "only the Strikes category's two registered actions appear"
            );

            for entity in app
                .world_mut()
                .query_filtered::<Entity, With<ActionButton>>()
                .iter(app.world())
                .collect::<Vec<_>>()
            {
                let node = app.world().get::<Node>(entity).expect("action node");
                assert_meets_touch_target(node, "phone action button");
            }
        }

        #[test]
        fn tapping_the_open_category_again_closes_it() {
            let mut app = mobile_test_app();
            let movement_button = find_category_button(&mut app, ActionCategory::Movement);
            press_button(&mut app, movement_button);
            assert_eq!(action_button_ids(&mut app).len(), 3, "Movement opened");

            press_button(&mut app, movement_button);
            let buttons = app
                .world_mut()
                .query_filtered::<(), With<ActionButton>>()
                .iter(app.world())
                .count();
            assert_eq!(buttons, 0, "tapping the open category again closes it");
        }

        #[test]
        fn tapping_a_different_category_switches_directly() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            assert_eq!(
                action_button_ids(&mut app),
                vec!["heavy-strike", "quick-strike"]
            );

            let defense_button = find_category_button(&mut app, ActionCategory::Defense);
            press_button(&mut app, defense_button);
            assert_eq!(
                action_button_ids(&mut app),
                vec!["block"],
                "switching to Defense shows only its action, never both categories at once"
            );
        }

        #[test]
        fn selecting_a_phone_action_emits_the_same_command_as_desktop() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            drain_enemy_stamina(&mut app);

            let quick_strike = find_button(&mut app, "quick-strike");
            press_button(&mut app, quick_strike);
            advance_presentation(&mut app);
            advance_presentation(&mut app);

            assert_eq!(
                enemy_pools(&mut app),
                (64, 20),
                "the phone action resolves the exact same combat command desktop's does"
            );
            assert_eq!(turn(&app).side, CombatSide::Player);
        }

        #[test]
        fn opening_and_closing_a_category_preserves_duel_state() {
            let mut app = mobile_test_app();
            let before = (player_pools(&mut app), enemy_pools(&mut app), turn(&app));

            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            press_button(&mut app, strikes_button); // close again

            let after = (player_pools(&mut app), enemy_pools(&mut app), turn(&app));
            assert_eq!(
                before, after,
                "opening/closing a category must not respawn fighters or change the turn"
            );
        }

        #[test]
        fn disabled_phone_actions_grey_out_and_show_a_reason() {
            let mut app = mobile_test_app();
            set_player_stamina(&mut app, 10);
            app.update();

            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);

            let heavy = find_button(&mut app, "heavy-strike");
            assert!(
                app.world().entity(heavy).contains::<DisabledButton>(),
                "heavy strike greys out below its 15 cost on phone too"
            );
            let reason = find_cost_or_reason_text(&mut app, heavy);
            assert_eq!(reason, "Stamina insuficientă (nevoie 15).");
        }

        #[test]
        fn a_test_registered_descriptor_opens_under_its_own_category() {
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
            app.insert_resource(ViewportInfo {
                width: 390.0,
                height: 844.0,
                is_mobile: true,
            });
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

            // The Special category now has one member, so a fifth category
            // control appears -- proof membership is fully descriptor-driven,
            // not a hard-coded four-category assumption.
            let special_button = find_category_button(&mut app, ActionCategory::Special);
            press_button(&mut app, special_button);
            assert_eq!(action_button_ids(&mut app), vec!["test-extra-action"]);
        }

        #[test]
        fn crossing_into_mobile_at_runtime_rebuilds_categories_from_the_flat_row() {
            let mut app = test_app();
            assert_eq!(action_button_ids(&mut app).len(), 7, "starts desktop-flat");

            app.world_mut()
                .resource_mut::<ViewportInfo>()
                .set_if_neq(ViewportInfo {
                    width: 390.0,
                    height: 844.0,
                    is_mobile: true,
                });
            app.update();

            let buttons = app
                .world_mut()
                .query_filtered::<(), With<ActionButton>>()
                .iter(app.world())
                .count();
            assert_eq!(buttons, 0, "closed category disclosure, no flat row left");
            let categories = app
                .world_mut()
                .query_filtered::<(), With<CategoryButton>>()
                .iter(app.world())
                .count();
            assert_eq!(categories, 4);
        }

        #[test]
        fn crossing_back_to_desktop_at_runtime_restores_the_flat_seven_button_row() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            assert_eq!(action_button_ids(&mut app).len(), 2, "opened on phone");

            app.world_mut()
                .resource_mut::<ViewportInfo>()
                .set_if_neq(ViewportInfo::default());
            app.update();

            assert_eq!(
                action_button_ids(&mut app).len(),
                7,
                "back to the full desktop row"
            );
            let categories = app
                .world_mut()
                .query_filtered::<(), With<CategoryButton>>()
                .iter(app.world())
                .count();
            assert_eq!(categories, 0, "no category controls on desktop");
        }

        #[test]
        fn a_category_left_open_does_not_survive_into_the_next_fight() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            assert_eq!(
                app.world().resource::<PhonePaletteState>().open,
                Some(ActionCategory::Strikes)
            );

            // Leave the fight mid-disclosure and start the next one.
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(GameState::FightResult);
            app.update();
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(GameState::Fight);
            app.update();

            assert_eq!(
                app.world().resource::<PhonePaletteState>().open,
                None,
                "every fight starts with no category open"
            );
            let buttons = app
                .world_mut()
                .query_filtered::<(), With<ActionButton>>()
                .iter(app.world())
                .count();
            assert_eq!(buttons, 0, "the new fight's action row starts empty");
        }
    }

    // --- descriptor-driven keyboard/gamepad focus (#213) ---

    mod focus_navigation {
        use super::*;

        fn press_key_and_settle(app: &mut App, key: KeyCode) {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(key);
            keys.clear();
        }

        fn focused_action_id(app: &mut App) -> Option<ActionId> {
            let focus = app.world().resource::<InputFocus>().get()?;
            app.world().get::<ActionButton>(focus).map(|b| b.id)
        }

        /// #216: whether the HUD's ⏸ button (its own `TabGroup::new(-1)`,
        /// ordered before the palette's `TabGroup::new(0)` to match its
        /// top-of-screen visual position) is currently focused.
        fn focused_is_pause_button(app: &mut App) -> bool {
            let Some(focus) = app.world().resource::<InputFocus>().get() else {
                return false;
            };
            app.world()
                .get::<crate::combat::pause::PauseButton>(focus)
                .is_some()
        }

        fn focused_category(app: &mut App) -> Option<ActionCategory> {
            let focus = app.world().resource::<InputFocus>().get()?;
            app.world().get::<CategoryButton>(focus).map(|b| b.category)
        }

        fn set_focus(app: &mut App, entity: Entity) {
            app.world_mut()
                .insert_resource(InputFocus::from_entity(entity));
        }

        fn spawn_gamepad(app: &mut App) -> Entity {
            app.world_mut().spawn(Gamepad::default()).id()
        }

        fn press_gamepad_and_settle(app: &mut App, gamepad: Entity, button: GamepadButton) {
            app.world_mut()
                .get_mut::<Gamepad>(gamepad)
                .unwrap()
                .digital_mut()
                .press(button);
            app.update();
            let mut gp = app.world_mut().get_mut::<Gamepad>(gamepad).unwrap();
            gp.digital_mut().release(button);
            gp.digital_mut().clear();
        }

        #[test]
        fn desktop_tab_order_matches_the_seven_visible_buttons_left_to_right() {
            let mut app = test_app();

            // #216: the HUD's ⏸ button is its own `TabGroup::new(-1)`,
            // ordered before the palette's `TabGroup::new(0)` to match its
            // top-of-screen visual position, so it is reached first.
            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(
                focused_is_pause_button(&mut app),
                "the HUD's ⏸ button must be reachable first, above the palette"
            );

            let expected = [
                "quick-strike",
                "heavy-strike",
                "block",
                "rest",
                "step-forward",
                "step-back",
                "leap-forward",
            ];
            let mut seen = Vec::new();
            for _ in 0..expected.len() {
                press_key_and_settle(&mut app, KeyCode::Tab);
                seen.push(focused_action_id(&mut app).expect("a button is focused"));
            }
            assert_eq!(
                seen, expected,
                "tab order follows ALL_ACTIONS' visual order"
            );

            // The next Tab wraps back to the ⏸ button.
            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(
                focused_is_pause_button(&mut app),
                "tab order wraps back to the ⏸ button, the first stop"
            );
        }

        #[test]
        fn phone_closed_tab_order_visits_only_the_four_category_buttons() {
            let mut app = mobile_test_app();

            // #216: the HUD's ⏸ button precedes the palette's own group.
            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(focused_is_pause_button(&mut app));

            let expected = [
                ActionCategory::Strikes,
                ActionCategory::Defense,
                ActionCategory::Movement,
                ActionCategory::Utility,
            ];
            let mut seen = Vec::new();
            for _ in 0..expected.len() {
                press_key_and_settle(&mut app, KeyCode::Tab);
                seen.push(focused_category(&mut app).expect("a category is focused"));
                assert_eq!(
                    focused_action_id(&mut app),
                    None,
                    "no action button exists while every category is closed"
                );
            }
            assert_eq!(seen, expected, "categories tab in CATEGORY_ORDER");

            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(
                focused_is_pause_button(&mut app),
                "tab order wraps back to the ⏸ button"
            );
        }

        #[test]
        fn phone_open_tab_order_visits_the_open_actions_then_every_category() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);

            // #216: the HUD's ⏸ button precedes the palette's own group.
            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(focused_is_pause_button(&mut app));

            let expected_actions = ["quick-strike", "heavy-strike"];
            let expected_categories = [
                ActionCategory::Strikes,
                ActionCategory::Defense,
                ActionCategory::Movement,
                ActionCategory::Utility,
            ];

            for expected in expected_actions {
                press_key_and_settle(&mut app, KeyCode::Tab);
                assert_eq!(focused_action_id(&mut app), Some(expected));
            }
            for expected in expected_categories {
                press_key_and_settle(&mut app, KeyCode::Tab);
                assert_eq!(focused_category(&mut app), Some(expected));
            }
            // Wraps back to the ⏸ button, the first stop.
            press_key_and_settle(&mut app, KeyCode::Tab);
            assert!(focused_is_pause_button(&mut app));
        }

        #[test]
        fn closing_a_category_moves_focus_to_its_own_category_button() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);

            let quick_strike = find_button(&mut app, "quick-strike");
            set_focus(&mut app, quick_strike);

            // Close Strikes again: its action row despawns, focus must move
            // to the still-alive Strikes category button, not be left
            // dangling on the despawned action button.
            press_button(&mut app, strikes_button);

            assert_eq!(
                app.world().resource::<InputFocus>().get(),
                Some(strikes_button)
            );
        }

        #[test]
        fn switching_categories_moves_focus_to_the_just_closed_categorys_button() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);

            let quick_strike = find_button(&mut app, "quick-strike");
            set_focus(&mut app, quick_strike);

            let defense_button = find_category_button(&mut app, ActionCategory::Defense);
            press_button(&mut app, defense_button);

            // The safe neighbor is the category whose actions just
            // disappeared (Strikes), not the newly opened one (Defense).
            assert_eq!(
                app.world().resource::<InputFocus>().get(),
                Some(strikes_button),
                "focus lands on the just-closed category's own button"
            );
        }

        #[test]
        fn focus_left_on_a_category_button_survives_opening_a_different_category() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            let defense_button = find_category_button(&mut app, ActionCategory::Defense);
            set_focus(&mut app, defense_button);

            // Opening Strikes via the mouse must not disturb focus that was
            // never on a despawned control in the first place.
            press_button(&mut app, strikes_button);
            assert_eq!(
                app.world().resource::<InputFocus>().get(),
                Some(defense_button)
            );
        }

        #[test]
        fn selecting_a_focused_enabled_action_via_keyboard_emits_the_command() {
            let mut app = test_app();
            drain_enemy_stamina(&mut app);
            let quick_strike = find_button(&mut app, "quick-strike");
            set_focus(&mut app, quick_strike);

            press_key_and_settle(&mut app, KeyCode::Enter);
            advance_presentation(&mut app);
            advance_presentation(&mut app);

            assert_eq!(
                enemy_pools(&mut app),
                (64, 20),
                "Enter on the focused quick-strike button resolves the same command a click does"
            );
            assert_eq!(turn(&app).side, CombatSide::Player);
        }

        #[test]
        fn selecting_a_focused_disabled_action_via_keyboard_never_emits() {
            let mut app = test_app();
            set_player_stamina(&mut app, 10);
            app.update();
            let heavy = find_button(&mut app, "heavy-strike");
            set_focus(&mut app, heavy);

            let before = (player_pools(&mut app), enemy_pools(&mut app));
            press_key_and_settle(&mut app, KeyCode::Enter);
            app.update();

            assert_eq!(
                (player_pools(&mut app), enemy_pools(&mut app)),
                before,
                "a disabled focused action must not emit on Enter"
            );
            assert_eq!(turn(&app).side, CombatSide::Player, "turn did not pass");
        }

        #[test]
        fn selecting_a_focused_category_via_gamepad_opens_it() {
            let mut app = mobile_test_app();
            let gamepad = spawn_gamepad(&mut app);
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            set_focus(&mut app, strikes_button);

            press_gamepad_and_settle(&mut app, gamepad, GamepadButton::South);

            assert_eq!(
                action_button_ids(&mut app),
                vec!["heavy-strike", "quick-strike"],
                "gamepad South on the focused Strikes button opens it, same as a tap"
            );
        }

        #[test]
        fn selecting_a_focused_disabled_action_via_gamepad_never_emits() {
            let mut app = test_app();
            set_player_stamina(&mut app, 10);
            app.update();
            let gamepad = spawn_gamepad(&mut app);
            let heavy = find_button(&mut app, "heavy-strike");
            set_focus(&mut app, heavy);

            let before = (player_pools(&mut app), enemy_pools(&mut app));
            press_gamepad_and_settle(&mut app, gamepad, GamepadButton::South);
            app.update();

            assert_eq!(
                (player_pools(&mut app), enemy_pools(&mut app)),
                before,
                "a disabled focused action must not emit on gamepad South either"
            );
            assert_eq!(turn(&app).side, CombatSide::Player, "turn did not pass");
        }

        #[test]
        fn a_test_registered_descriptor_participates_in_tab_order_with_no_palette_specific_code() {
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

            // #216: the HUD's ⏸ button now also shares tab order with the
            // palette (its own `TabGroup::new(-1)`, before the palette's
            // `0`), so a blind walk must tolerate landing on it (no
            // `ActionButton`, so `focused_action_id` is `None` there)
            // instead of assuming every stop is an action button.
            for _ in 0..9 {
                press_key_and_settle(&mut app, KeyCode::Tab);
            }
            let visited: Vec<ActionId> = (0..9)
                .filter_map(|_| {
                    press_key_and_settle(&mut app, KeyCode::Tab);
                    focused_action_id(&mut app)
                })
                .collect();
            assert!(
                visited.contains(&"test-extra-action"),
                "the registered eighth descriptor's button must be reachable by Tab: {visited:?}"
            );
        }
    }
}
