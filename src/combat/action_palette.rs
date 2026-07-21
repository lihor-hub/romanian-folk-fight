//! Combat action palette (#189/#199, children of #143): renders the desktop
//! action bar and the phone category-disclosure palette entirely from
//! [`ActionDescriptor`]s. `hud::spawn_hud` delegates the action-bar subtree
//! to [`spawn_action_bar`] below instead of hard-coding buttons; every
//! subsequent frame, [`update_action_buttons`] re-derives the same
//! descriptors from live duel state and reconciles the already-spawned
//! buttons against them.
//!
//! ## Desktop (combat redesign §3, replacing #189's flat strip)
//!
//! A vertical command banner on the stage's left edge
//! ([`spawn_desktop_banner`]): an embroidered-linen column holding the four
//! labeled groups of [`BANNER_CATEGORY_ORDER`] in decision order, one
//! pictogram-led row per descriptor. Reach-disabled strike rows show a small
//! distance mark and, while hovered, pulse the arena's ground distance chip
//! ([`pulse_distance_chip_on_reach_hover`]).
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

use std::collections::HashMap;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::arena::GROUND_CHIP_ALPHA;
use crate::arena::GroundDistanceChip;
use crate::character::{Attributes, EnemyFighter, PlayerFighter, Stamina};
use crate::core::{LetterboxRect, UiFont, ViewportInfo};
use crate::menu::DisabledButton;
use crate::settings::AccessibilityPreferences;
use crate::theme::{
    ACTION_BUTTON_TOUCH_TARGET, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED,
    CREAM, GOLD, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
use crate::ui_widgets::focus::{Focusable, TabGroup, TabIndex, redirect_focus_if_inside};

use super::actions::{
    ActionCategory, ActionCost, ActionDescriptor, ActionId, DescriptorContext, ExtraDescriptors,
    action_id, category_label, generate_action_descriptors, group_by_category,
};
use super::engine::CombatAction;
use super::hud::{ActionBarRoot, HudScreen};
use super::systems::{CombatPresentation, CombatTurn, PlayerActionEvent};

#[cfg(test)]
use crate::theme::PANEL_BORDER_INSET;

const ACTION_BAR_PADDING: f32 = 8.0;

/// Width of the desktop command banner's embroidered-linen column (combat
/// redesign §3, `docs/combat-redesign-proposal.md`).
const BANNER_WIDTH: f32 = 200.0;
/// Left/bottom margin anchoring the banner to the stage's lower-left corner.
const BANNER_MARGIN: f32 = 16.0;
/// Hard cap on the banner's height, as a percentage of the letterboxed
/// stage (§3: "height to ~65% of stage"); `banner_occupied_height`'s test
/// proves the nominal eight-row content stays inside it.
const BANNER_MAX_HEIGHT_PERCENT: f32 = 65.0;
/// Side of a banner row's square pictogram tile. 28 (not the proposal's
/// sketched ~40) so eight rows plus four group headers fit the 65% height
/// budget inside `panel_bundle`'s 24px border inset; the 32px source
/// pictograms downscale cleanly.
const BANNER_TILE_SIZE: f32 = 28.0;
/// Minimum height of one banner action row (the tile side: the two text
/// lines beside it are shorter).
const BANNER_ROW_HEIGHT: f32 = 28.0;
/// Gap between a group's header and its rows, and between sibling rows.
const BANNER_ROW_GAP: f32 = 2.0;
/// Gap between two labeled groups.
const BANNER_GROUP_GAP: f32 = 8.0;
/// Fixed height of a group header line, so the banner's occupied height is
/// a pure function of these constants (see `banner_occupied_height`).
const BANNER_HEADER_HEIGHT: f32 = 14.0;
/// Side of the phone tiles' square pictogram, sized to fit the 56px rows
/// alongside their two text lines.
const PHONE_TILE_SIZE: f32 = 20.0;
/// Side of the small square distance mark a reach-disabled strike row shows
/// (see [`ReachDistanceMark`]).
const REACH_MARK_SIZE: f32 = 8.0;

/// Row height for every phone control — category buttons and open-category
/// action buttons alike (#199) — comfortably above the 44px CSS touch-target
/// floor the issue requires (also above [`ACTION_BUTTON_TOUCH_TARGET`], the
/// pre-#199 mobile minimum, so this is never a shrink).
const PHONE_TARGET_HEIGHT: f32 = 56.0;
/// Gap between phone controls in the same row, and between the category row
/// and the (when open) action row above it.
const PHONE_ROW_GAP: f32 = 8.0;

/// Clearance the phone action bar's container keeps above the real window's
/// bottom edge (#276) -- matches the pre-#276 flat `bottom: Val::Px(8.0)`
/// [`spawn_phone_action_bar`] used to hard-code, so a viewport with no
/// letterbox slack below the stage (see [`phone_bar_bottom_offset`]) falls
/// back to exactly the old placement instead of a new, untested magic
/// number.
const PHONE_BAR_WINDOW_MARGIN: f32 = 8.0;

/// #276's single explicit mobile layout contract: the `bottom` (`Val::Px`,
/// relative to [`HudScreen`] -- see `hud::hud_root_node`'s doc comment)
/// [`spawn_phone_action_bar`]'s container must use so that even its
/// tallest state -- both rows open -- never grows *into* the letterboxed
/// arena stage at all, and therefore can never cover the fighters' bodies or
/// [`super::hud::LogPanelRoot`]'s combat log, both of which (like every
/// full-window HUD overlay, #125) stay confined inside the stage.
///
/// On a real phone, [`ViewportInfo`]'s width is far smaller than its height,
/// so the fixed 4:3 [`LetterboxRect`] (#125) occupies only a thin band
/// roughly in the screen's vertical middle, leaving a large strip of
/// background below it (and above it) the arena camera never draws into --
/// see this module's `mobile_layout` test module for the exact 390x844
/// numbers this issue was filed against. That strip is precisely the #276
/// bug's root cause: pre-#276, [`spawn_phone_action_bar`] anchored
/// `bottom: Val::Px(8.0)` against the stage's *own* bottom edge, so opening a
/// category grew the bar upward *into* that same thin band the fighters and
/// the combat log already occupy -- even the closed, single-row bar already
/// reached into it. This function instead anchors the bar against the real
/// window's bottom edge whenever the stage doesn't already reach it,
/// reserving exactly the otherwise-unused strip below the stage for the bar,
/// so opening a category grows into space nothing else was ever drawing
/// into, rather than the arena.
///
/// Falls back to the flat [`PHONE_BAR_WINDOW_MARGIN`] when the viewport has
/// no such slack (`letterbox.size.y` already spans the window's full height
/// -- a squarish or landscape-shaped "mobile-width" window, which
/// `is_mobile_width`'s width-only breakpoint does not itself rule out): in
/// that edge case there is no unused strip to reserve, so this keeps the
/// pre-#276 placement rather than guessing at a different one.
pub(super) fn phone_bar_bottom_offset(viewport: ViewportInfo, letterbox: LetterboxRect) -> f32 {
    let stage_bottom = letterbox.position.y + letterbox.size.y;
    let below_stage_room = (viewport.height - stage_bottom).max(0.0);
    PHONE_BAR_WINDOW_MARGIN - below_stage_room
}

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

/// Marker on every desktop banner action row (§3): tells
/// [`update_action_buttons`] to render the compact [`banner_info_line`]
/// instead of the phone's [`ActionDescriptor::sublabel`] while enabled.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BannerActionRow;

/// Marker on one banner group's header text, carrying its category so tests
/// can assert the §3 decision order without matching on Romanian strings.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BannerGroupHeader(pub ActionCategory);

/// The small square mark at the tail of a banner strike row, shown only
/// while the strike is reach-disabled: a miniature echo of the arena's
/// ground distance chip (same dim [`TEXT_DISABLED`] tone), tying the row's
/// "Prea departe" reason to the on-stage gap readout. Hovering the row
/// additionally pulses the chip itself — see
/// [`pulse_distance_chip_on_reach_hover`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReachDistanceMark;

/// Handles to the generated action pictograms
/// (`assets/ui/pictograms/<pictogram_id>.png`, `scripts/generate-pictograms.py`),
/// keyed by [`ActionDescriptor::pictogram_id`] — the exact string contract
/// [`super::actions::ActionId`]'s docs promised for #122. Loaded once at
/// startup; empty when no `AssetServer` exists (headless tests), in which
/// case every tile falls back to its ASCII [`glyph_for`] well.
#[derive(Resource, Debug, Clone, Default)]
pub(super) struct ActionPictograms(HashMap<ActionId, Handle<Image>>);

impl ActionPictograms {
    fn handle(&self, pictogram_id: ActionId) -> Option<Handle<Image>> {
        self.0.get(pictogram_id).cloned()
    }
}

/// Loads the eight real actions' pictograms. A descriptor registered via
/// [`ExtraDescriptors`] has no entry here and renders the ASCII fallback,
/// exactly like a missing file would.
pub(super) fn load_action_pictograms(
    mut icons: ResMut<ActionPictograms>,
    asset_server: Option<Res<AssetServer>>,
) {
    let Some(asset_server) = asset_server else {
        return;
    };
    for action in super::actions::ALL_ACTIONS {
        let id = action_id(action);
        icons
            .0
            .insert(id, asset_server.load(format!("ui/pictograms/{id}.png")));
    }
}

/// The desktop banner's §3 decision order — strikes, movement, defense,
/// recovery — deliberately different from phone's
/// [`super::actions::CATEGORY_ORDER`]: the banner reads top-to-bottom as
/// "what do I want to do this turn", while the phone's category strip keeps
/// its established attack-first disclosure order.
const BANNER_CATEGORY_ORDER: [ActionCategory; 5] = [
    ActionCategory::Strikes,
    ActionCategory::Movement,
    ActionCategory::Defense,
    ActionCategory::Utility,
    ActionCategory::Special,
];

/// The banner's Romanian group headers (§3). "Lovituri" (strikes as a
/// group of blows), not the phone's "Atac" — the proposal names the groups
/// explicitly.
fn banner_category_label(category: ActionCategory) -> &'static str {
    match category {
        ActionCategory::Strikes => "Lovituri",
        ActionCategory::Movement => "Mișcare",
        ActionCategory::Defense => "Apărare",
        ActionCategory::Utility => "Refacere",
        ActionCategory::Special => "Special",
    }
}

/// Groups `descriptors` in [`BANNER_CATEGORY_ORDER`], skipping empty
/// categories — same membership rule as
/// [`super::actions::group_by_category`] (always
/// [`ActionDescriptor::category`], so a test-registered descriptor lands in
/// its declared group automatically), only the display order differs.
fn banner_groups(descriptors: &[ActionDescriptor]) -> Vec<(ActionCategory, Vec<ActionDescriptor>)> {
    BANNER_CATEGORY_ORDER
        .into_iter()
        .filter_map(|category| {
            let members: Vec<ActionDescriptor> = descriptors
                .iter()
                .filter(|d| d.category == category)
                .cloned()
                .collect();
            (!members.is_empty()).then_some((category, members))
        })
        .collect()
}

/// A banner row's compact info line while enabled (§3): strikes show
/// `"70% · -9"`, block `"-3"`, rest `"+20"`, movement a direction arrow
/// with its band shift. Every number comes from the descriptor's own
/// structured fields ([`ActionDescriptor::hit_chance`]/
/// [`ActionDescriptor::cost`]); only the movement arrow is an id-keyed
/// cosmetic, like [`glyph_for`].
fn banner_info_line(descriptor: &ActionDescriptor) -> String {
    match descriptor.hit_chance {
        Some(chance) => format!("{chance}% · {}", banner_cost_line(descriptor)),
        None => banner_cost_line(descriptor),
    }
}

/// The chance-free part of [`banner_info_line`] — also the spawn-time text,
/// for the same reason [`spawn_action_bar`]'s placeholder spawn shows cost
/// only: the hit chance depends on both fighters' real `Attributes`.
fn banner_cost_line(descriptor: &ActionDescriptor) -> String {
    match descriptor.cost {
        ActionCost::Stamina(n) => format!("-{n}"),
        ActionCost::Restore(n) => format!("+{n}"),
        ActionCost::None => movement_hint(descriptor.pictogram_id).to_string(),
        other => other.display_text(),
    }
}

/// Direction arrow + band shift for a movement row's info line. Id-keyed
/// cosmetic text (fallback for unknown ids), mirroring [`glyph_for`].
fn movement_hint(pictogram_id: ActionId) -> &'static str {
    match pictogram_id {
        "step-forward" => "-> o bandă",
        "leap-forward" => ">> două benzi",
        "step-back" => "<- o bandă",
        _ => "poziție",
    }
}

/// The action palette: desktop's vertical command banner on the stage's
/// left edge (combat redesign §3) or phone's category-disclosure stack
/// (#199, see [`spawn_phone_action_bar`]). Both iterate
/// [`generate_action_descriptors`] plus [`ExtraDescriptors`] — never a
/// hard-coded button list — so a later registered action renders here with
/// no edits to this function.
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
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_action_bar(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
    extra: &ExtraDescriptors,
    icons: &ActionPictograms,
    viewport: &ViewportInfo,
    letterbox: &LetterboxRect,
) {
    if is_mobile {
        // The phone bar itself holds only text category buttons; its action
        // rows (and their pictogram tiles) spawn later, on category open —
        // see `sync_phone_open_category`.
        spawn_phone_action_bar(parent, ui_font, extra, viewport, letterbox);
        return;
    }
    spawn_desktop_banner(parent, ui_font, panel_texture, extra, icons);
}

/// §3's desktop command banner: a ~200px embroidered-linen column anchored
/// to the stage's lower-left corner (`left`/`bottom` [`BANNER_MARGIN`],
/// capped at [`BANNER_MAX_HEIGHT_PERCENT`] of the letterboxed stage the HUD
/// root is sized to), holding the four labeled groups of
/// [`BANNER_CATEGORY_ORDER`] in decision order, one [`spawn_banner_row`]
/// per descriptor. The staging clamp (`arena::staging::STAGE_MIN_X`)
/// guarantees fighters never walk more than a sliver behind it.
///
/// Tab order (#213): the single [`TabGroup`] walks the tree in spawn order,
/// so keyboard focus follows the same group-by-group decision order the eye
/// does.
fn spawn_desktop_banner(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    extra: &ExtraDescriptors,
    icons: &ActionPictograms,
) {
    let node = Node {
        position_type: PositionType::Absolute,
        left: Val::Px(BANNER_MARGIN),
        bottom: Val::Px(BANNER_MARGIN),
        width: Val::Px(BANNER_WIDTH),
        max_height: Val::Percent(BANNER_MAX_HEIGHT_PERCENT),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(BANNER_GROUP_GAP),
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
            // #213: one shared focus region for the whole banner — see
            // `crate::ui_widgets::focus`'s registration API.
            TabGroup::new(0),
        ))
        .with_children(|banner| {
            for (category, members) in banner_groups(&descriptors) {
                banner
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(BANNER_ROW_GAP),
                        ..default()
                    })
                    .with_children(|group| {
                        group
                            .spawn(Node {
                                height: Val::Px(BANNER_HEADER_HEIGHT),
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|header| {
                                header.spawn((
                                    Text::new(banner_category_label(category)),
                                    ui_font.text_font_bold(11.0),
                                    TextColor(GOLD),
                                    BannerGroupHeader(category),
                                ));
                            });
                        for descriptor in &members {
                            spawn_banner_row(group, descriptor, ui_font, icons);
                        }
                    });
            }
        });
}

/// One banner action row (§3): pictogram tile, then the Romanian label over
/// its compact info line, then the (hidden by default) reach mark. The row
/// is the [`ActionButton`]; [`update_action_buttons`] drives its
/// enabled/dimmed state and swaps the info line for the descriptor's
/// [`ActionDescriptor::disabled_reason`] exactly like every other palette
/// button.
fn spawn_banner_row(
    parent: &mut ChildSpawnerCommands,
    descriptor: &ActionDescriptor,
    ui_font: &UiFont,
    icons: &ActionPictograms,
) {
    parent
        .spawn((
            Button,
            ActionButton {
                id: descriptor.id,
                intent: descriptor.intent,
            },
            BannerActionRow,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(BANNER_ROW_HEIGHT),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::horizontal(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(BUTTON_NORMAL),
            // #213: disabled actions stay focusable so their reason is
            // reachable — see `crate::ui_widgets::focus`'s registration API.
            Focusable,
            TabIndex(0),
        ))
        .with_children(|row| {
            spawn_pictogram_tile(
                row,
                descriptor.pictogram_id,
                icons,
                ui_font,
                BANNER_TILE_SIZE,
            );
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|text_column| {
                text_column.spawn((
                    Text::new(descriptor.label),
                    ui_font.text_font(12.0),
                    TextColor(CREAM),
                ));
                // Cost only at spawn (placeholder attributes, see
                // `banner_cost_line`); `update_action_buttons` swaps in the
                // full `banner_info_line` the same frame.
                text_column.spawn((
                    Text::new(banner_cost_line(descriptor)),
                    ui_font.text_font(10.0),
                    TextColor(CREAM),
                    ActionCostOrReason,
                ));
            });
            row.spawn((
                Node {
                    width: Val::Px(REACH_MARK_SIZE),
                    height: Val::Px(REACH_MARK_SIZE),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(TEXT_DISABLED.with_alpha(GROUND_CHIP_ALPHA)),
                Visibility::Hidden,
                ReachDistanceMark,
            ));
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
///
/// `bottom` (#276) comes from [`phone_bar_bottom_offset`] instead of a flat
/// constant -- see that function's doc comment for why: on a real phone this
/// anchors the whole bar against the *real window's* bottom edge, in the
/// otherwise-unused strip below the letterboxed stage, instead of the
/// stage's own bottom edge the pre-#276 bar grew into.
fn spawn_phone_action_bar(
    parent: &mut ChildSpawnerCommands,
    ui_font: &UiFont,
    extra: &ExtraDescriptors,
    viewport: &ViewportInfo,
    letterbox: &LetterboxRect,
) {
    let node = Node {
        position_type: PositionType::Absolute,
        bottom: Val::Px(phone_bar_bottom_offset(*viewport, *letterbox)),
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

/// The square pictogram tile shared by every action row — banner
/// ([`BANNER_TILE_SIZE`]) and phone ([`PHONE_TILE_SIZE`]) alike, so the two
/// layouts can never drift on how they render an action's icon. Renders the
/// generated pictogram when [`ActionPictograms`] has a handle for
/// `pictogram_id`, and falls back to the carved-wood ASCII [`glyph_for`]
/// well otherwise (headless tests without an `AssetServer`, or a descriptor
/// registered without art).
fn spawn_pictogram_tile(
    parent: &mut ChildSpawnerCommands,
    pictogram_id: ActionId,
    icons: &ActionPictograms,
    ui_font: &UiFont,
    size: f32,
) {
    let node = Node {
        width: Val::Px(size),
        height: Val::Px(size),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    if let Some(handle) = icons.handle(pictogram_id) {
        parent.spawn((node, ImageNode::new(handle), ActionGlyph));
        return;
    }
    parent
        .spawn((
            node,
            BackgroundColor(WALNUT),
            BorderColor::all(GOLD),
            ActionGlyph,
        ))
        .with_children(|well| {
            well.spawn((
                Text::new(glyph_for(pictogram_id)),
                ui_font.text_font_bold(12.0),
                TextColor(GOLD),
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
    icons: &ActionPictograms,
    enabled: bool,
) {
    let (background, text_color) = if enabled {
        (BUTTON_NORMAL, CREAM)
    } else {
        (BUTTON_DISABLED, TEXT_DISABLED)
    };
    let cost_or_reason = if enabled {
        // #124: `descriptor` here always carries the real, live-queried
        // `Attributes` for both fighters (this button only spawns once a
        // category is opened, from `sync_phone_open_category`'s
        // `live_descriptors` — never the bar's cosmetic spawn-time
        // placeholder), so `sublabel()`'s hit chance is always correct.
        descriptor.sublabel()
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
        spawn_pictogram_tile(
            button,
            descriptor.pictogram_id,
            icons,
            ui_font,
            PHONE_TILE_SIZE,
        );
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
        "normal-strike" => "=>",
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
    Has<BannerActionRow>,
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
/// "expose their reason" acceptance criterion). The background/greyed-marker
/// swap below only touches buttons whose enabled state actually flipped, so
/// it does not fight the hover-feedback system — the exact cadence the
/// pre-#189 HUD already used for color alone. Applies identically to
/// desktop's eight buttons and phone's (0–3) open action-row buttons — both
/// carry the same [`ActionButton`] component.
///
/// The cost-or-reason *text* itself is resynced every call regardless of
/// that flip (#124), unlike before: the stamina/restore cost is fixed per
/// action, but the two strikes' hit-chance line depends on both fighters'
/// `Attributes` — which the desktop bar's cosmetic spawn-time placeholder
/// (`DescriptorContext::spawn_placeholder`, see [`spawn_action_bar`]) has no
/// real values for. Resyncing unconditionally corrects that placeholder text
/// as soon as this system runs with real duel state, the same frame `OnEnter`
/// spawned the bar and before anything is rendered — the same guarantee
/// `spawn_placeholder`'s docs already describe for cost text, extended to
/// hit chance — rather than waiting on an enabled/disabled flip that may
/// never happen for a button that starts (and stays) enabled.
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
    mut reach_marks: Query<&mut Visibility, With<ReachDistanceMark>>,
    glyph_tiles: Query<(), With<ActionGlyph>>,
    child_children: Query<&Children>,
) {
    let has_turn = turn.is_some();
    let presentation_busy = presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy);
    let descriptors = live_descriptors(turn.as_deref(), presentation_busy, &player, &enemy, &extra);

    for (entity, button, was_disabled, is_banner_row, mut background, children) in &mut buttons {
        let Some(descriptor) = descriptors.iter().find(|d| d.id == button.id) else {
            continue;
        };
        // No `CombatTurn` yet (the fighters haven't spawned this frame):
        // matches the pre-#189 HUD's fallback of treating every action as
        // disabled rather than making a claim about state that isn't real
        // yet.
        let enabled = has_turn && descriptor.enabled;
        // Only the background/greyed-marker swap is gated on a real flip;
        // see this function's doc comment for why the text below is not.
        if enabled == was_disabled {
            if enabled {
                commands.entity(entity).remove::<DisabledButton>();
                background.0 = BUTTON_NORMAL;
            } else {
                commands.entity(entity).insert(DisabledButton);
                background.0 = BUTTON_DISABLED;
            }
        }
        let text_color = if enabled { CREAM } else { TEXT_DISABLED };
        let cost_or_reason_text = if !enabled {
            descriptor
                .disabled_reason
                .clone()
                .unwrap_or_else(|| "Lupta nu a început încă.".to_string())
        } else if is_banner_row {
            banner_info_line(descriptor)
        } else {
            descriptor.sublabel()
        };
        // §3: a reach-disabled strike additionally shows the small distance
        // mark tying its reason to the arena's ground gap chip — keyed off
        // the descriptor's own `position_legal`, never a re-derived rule.
        let mark_visibility = if descriptor.hit_chance.is_some() && !descriptor.position_legal {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for child in children.iter() {
            if let Ok(mut mark) = reach_marks.get_mut(child) {
                mark.set_if_neq(mark_visibility);
            }
            apply_row_text(child, text_color, &cost_or_reason_text, &mut text_nodes);
            // The banner row nests its label/info texts one level down, in
            // a column beside the pictogram tile — restyle those too. The
            // tile itself is skipped so its fallback ASCII glyph keeps its
            // carved-gold tone, exactly like the pre-banner glyph well.
            if glyph_tiles.contains(child) {
                continue;
            }
            if let Ok(grandchildren) = child_children.get(child) {
                for grandchild in grandchildren.iter() {
                    apply_row_text(
                        grandchild,
                        text_color,
                        &cost_or_reason_text,
                        &mut text_nodes,
                    );
                }
            }
        }
    }
}

/// Restyles one (grand)child text node of an action button: the shared
/// enabled/disabled color, plus the cost-or-reason swap on the
/// [`ActionCostOrReason`] node. No-op for non-text entities.
fn apply_row_text(
    entity: Entity,
    text_color: Color,
    cost_or_reason_text: &str,
    text_nodes: &mut Query<(&mut TextColor, Option<&mut Text>, Has<ActionCostOrReason>)>,
) {
    if let Ok((mut color, text, is_cost_or_reason)) = text_nodes.get_mut(entity) {
        if color.0 != text_color {
            color.0 = text_color;
        }
        if is_cost_or_reason
            && let Some(mut text) = text
            && text.0 != cost_or_reason_text
        {
            text.0 = cost_or_reason_text.to_string();
        }
    }
}

/// Cycle frequency of the ground-chip reach pulse, in hertz.
const CHIP_PULSE_HZ: f32 = 1.6;
/// Alpha the pulse adds to [`GROUND_CHIP_ALPHA`]: a guaranteed floor lift
/// plus the oscillating half, so the chip is always visibly brighter while
/// linked — never coincidentally at its resting value mid-wave.
const CHIP_PULSE_ALPHA_FLOOR: f32 = 0.15;
const CHIP_PULSE_ALPHA_WAVE: f32 = 0.15;
/// Scale the pulse adds to the chip's resting 1.0, same floor+wave split.
const CHIP_PULSE_SCALE_FLOOR: f32 = 0.03;
const CHIP_PULSE_SCALE_WAVE: f32 = 0.04;

/// §3's hover link: while a reach-disabled strike row is hovered, the
/// arena's ground distance chip ([`GroundDistanceChip`]) pulses subtly —
/// a small alpha/scale breath — so the player's eye is led from the greyed
/// row to the on-stage gap readout explaining it. Keyed off the live
/// descriptor's `hit_chance`/`position_legal` (the same
/// [`live_descriptors`] every palette system reads), never a second copy of
/// the reach rule. Under reduced motion the chip holds a static highlight
/// (alpha floor only, no oscillation or scaling) instead of animating.
/// Purely presentational: touches only the chip's `TextColor`/`Transform`,
/// never duel state or the combat RNG.
#[allow(clippy::too_many_arguments)]
pub(super) fn pulse_distance_chip_on_reach_hover(
    time: Res<Time>,
    accessibility: Option<Res<AccessibilityPreferences>>,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    extra: Res<ExtraDescriptors>,
    player: PlayerStats,
    enemy: EnemyStats,
    buttons: Query<(&Interaction, &ActionButton)>,
    mut chips: Query<(&mut TextColor, &mut Transform), With<GroundDistanceChip>>,
) {
    let hovered: Vec<ActionId> = buttons
        .iter()
        .filter(|(interaction, _)| **interaction == Interaction::Hovered)
        .map(|(_, button)| button.id)
        .collect();
    let linked = !hovered.is_empty() && {
        let presentation_busy = presentation
            .as_deref()
            .is_some_and(CombatPresentation::is_busy);
        live_descriptors(turn.as_deref(), presentation_busy, &player, &enemy, &extra)
            .iter()
            .any(|d| hovered.contains(&d.id) && d.hit_chance.is_some() && !d.position_legal)
    };

    let (alpha, scale) = if linked {
        let reduced_motion = accessibility.as_deref().is_some_and(|a| a.reduced_motion);
        if reduced_motion {
            (GROUND_CHIP_ALPHA + CHIP_PULSE_ALPHA_FLOOR, 1.0)
        } else {
            let wave =
                0.5 + 0.5 * (time.elapsed_secs() * CHIP_PULSE_HZ * std::f32::consts::TAU).sin();
            (
                GROUND_CHIP_ALPHA + CHIP_PULSE_ALPHA_FLOOR + CHIP_PULSE_ALPHA_WAVE * wave,
                1.0 + CHIP_PULSE_SCALE_FLOOR + CHIP_PULSE_SCALE_WAVE * wave,
            )
        }
    } else {
        (GROUND_CHIP_ALPHA, 1.0)
    };
    for (mut color, mut transform) in &mut chips {
        let target = TEXT_DISABLED.with_alpha(alpha);
        if color.0 != target {
            color.0 = target;
        }
        if transform.scale != Vec3::splat(scale) {
            transform.scale = Vec3::splat(scale);
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
    icons: Res<ActionPictograms>,
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
            spawn_phone_action_button(row, descriptor, &ui_font, &icons, enabled);
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
/// replaces the *entire* layout (eight flat buttons versus category
/// disclosure), so there is no single control on the new layout that is the
/// "same" one focus was on; clearing is the documented safe fallback here,
/// and the next Tab press lands on the new layout's first control (the same
/// behavior [`bevy::input_focus::tab_navigation::TabNavigation::navigate`]
/// already gives an unset focus).
#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild_action_bar_on_breakpoint_change(
    mut commands: Commands,
    viewport: Res<ViewportInfo>,
    letterbox: Res<LetterboxRect>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    extra: Res<ExtraDescriptors>,
    icons: Res<ActionPictograms>,
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
        spawn_action_bar(
            parent,
            &ui_font,
            &panel_texture,
            viewport.is_mobile,
            &extra,
            &icons,
            &viewport,
            &letterbox,
        );
    });
}

/// The banner's actual rendered padding after `panel_bundle` merges
/// `ACTION_BAR_PADDING` with the border inset (#120) — whichever is larger,
/// per side. Kept in sync with `merge_panel_padding`'s per-side rule so the
/// height budget below reflects reality instead of the pre-merge constant.
#[cfg(test)]
fn banner_effective_padding() -> f32 {
    ACTION_BAR_PADDING.max(PANEL_BORDER_INSET)
}

/// The banner's nominal occupied height for the eight real actions in four
/// groups, as a pure function of the layout constants: rows, headers, the
/// row gap after each header/between rows, the inter-group gaps, and the
/// merged panel padding. The geometry test proves this stays within
/// [`BANNER_MAX_HEIGHT_PERCENT`] of the 600px design stage.
#[cfg(test)]
fn banner_occupied_height() -> f32 {
    let rows = 8.0;
    let groups = 4.0;
    rows * BANNER_ROW_HEIGHT
        + groups * BANNER_HEADER_HEIGHT
        // Each group is a column with `BANNER_ROW_GAP` between its header
        // and every row: one gap per row.
        + rows * BANNER_ROW_GAP
        + (groups - 1.0) * BANNER_GROUP_GAP
        + banner_effective_padding() * 2.0
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
        atac: 1,
        aparare: 2,
        carisma: 1,
        magie: 0,
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
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
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
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
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
    fn spawn_placeholder_produces_all_eight_actions_with_no_disabled_reason() {
        let descriptors = generate_action_descriptors(&DescriptorContext::spawn_placeholder());
        assert_eq!(descriptors.len(), 8);
    }

    // --- headless screen behavior (moved from the pre-#189 hud.rs) ---

    #[test]
    fn entering_fight_spawns_all_eight_action_buttons() {
        let mut app = test_app();
        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            buttons, 8,
            "five combat buttons plus three movement buttons"
        );
    }

    #[test]
    fn banner_rows_are_icon_led_and_the_banner_is_anchored_left() {
        let mut app = test_app();

        let glyphs = app
            .world_mut()
            .query::<&ActionGlyph>()
            .iter(app.world())
            .count();
        assert_eq!(glyphs, 8, "every action row has a pictogram tile");

        let node = app
            .world_mut()
            .query_filtered::<&Node, With<ActionBarRoot>>()
            .single(app.world())
            .expect("one banner root");
        assert_eq!(node.left, Val::Px(BANNER_MARGIN), "anchored left:16");
        assert_eq!(node.bottom, Val::Px(BANNER_MARGIN), "anchored bottom:16");
        assert_eq!(node.width, Val::Px(BANNER_WIDTH), "~200px wide column");
        assert_eq!(
            node.max_height,
            Val::Percent(BANNER_MAX_HEIGHT_PERCENT),
            "hard-capped at ~65% of the letterboxed stage"
        );
        assert_eq!(node.flex_direction, FlexDirection::Column);
    }

    /// §3's height budget, as pure geometry: the eight rows across four
    /// labeled groups — including headers, gaps, and the embroidered
    /// panel's merged padding — fit within 65% of the 600px design stage,
    /// so the `max_height` cap above never actually truncates the nominal
    /// content.
    #[test]
    fn banner_nominal_content_fits_the_65_percent_stage_height_budget() {
        const STAGE_DESIGN_HEIGHT: f32 = 600.0;
        assert!(
            banner_occupied_height() <= STAGE_DESIGN_HEIGHT * BANNER_MAX_HEIGHT_PERCENT / 100.0,
            "banner content ({}) must fit {}% of the {STAGE_DESIGN_HEIGHT}px stage",
            banner_occupied_height(),
            BANNER_MAX_HEIGHT_PERCENT
        );
    }

    /// Walks the banner subtree in tree order and returns what it renders:
    /// each group header's category followed by that group's action button
    /// ids — the exact §3 decision-order contract.
    fn banner_sequence(app: &mut App) -> Vec<String> {
        fn walk(app: &App, entity: Entity, out: &mut Vec<String>) {
            if let Some(header) = app.world().get::<BannerGroupHeader>(entity) {
                out.push(format!("header:{:?}", header.0));
            }
            if let Some(button) = app.world().get::<ActionButton>(entity) {
                out.push(button.id.to_string());
            }
            if let Some(children) = app.world().get::<Children>(entity) {
                for child in children.iter() {
                    walk(app, child, out);
                }
            }
        }
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<ActionBarRoot>>()
            .single(app.world())
            .expect("one banner root");
        let mut out = Vec::new();
        walk(app, root, &mut out);
        out
    }

    #[test]
    fn banner_groups_render_in_decision_order_with_their_rows() {
        let mut app = test_app();
        assert_eq!(
            banner_sequence(&mut app),
            vec![
                "header:Strikes",
                "quick-strike",
                "normal-strike",
                "heavy-strike",
                "header:Movement",
                "step-forward",
                "leap-forward",
                "step-back",
                "header:Defense",
                "block",
                "header:Utility",
                "rest",
            ],
            "groups follow §3's decision order, rows follow ALL_ACTIONS' relative order"
        );
    }

    #[test]
    fn banner_strike_rows_show_the_compact_info_line_when_enabled() {
        let mut app = test_app();
        let quick = find_button(&mut app, "quick-strike");
        let text = find_cost_or_reason_text(&mut app, quick);
        assert!(
            text.ends_with("% · -5") || text.contains("% · -5"),
            "banner quick-strike info line {text:?} must be the compact \"<hit>% · -5\" form"
        );
        let rest = find_button(&mut app, "rest");
        assert_eq!(find_cost_or_reason_text(&mut app, rest), "+20");
        let block = find_button(&mut app, "block");
        assert_eq!(find_cost_or_reason_text(&mut app, block), "-3");
        // Step-back is the movement action that is *enabled* at the
        // starting close range; the disabled advances show their reasons.
        let step_back = find_button(&mut app, "step-back");
        assert_eq!(find_cost_or_reason_text(&mut app, step_back), "<- o bandă");
    }

    /// Finds the [`ReachDistanceMark`] child of `button`.
    fn find_reach_mark(app: &mut App, button: Entity) -> Entity {
        let children = app
            .world()
            .get::<Children>(button)
            .expect("button has children")
            .to_vec();
        children
            .into_iter()
            .find(|&child| app.world().get::<ReachDistanceMark>(child).is_some())
            .expect("banner row has a reach mark child")
    }

    #[test]
    fn reach_disabled_strikes_show_the_distance_mark_and_reason() {
        let mut app = test_app();
        app.world_mut().resource_mut::<CombatTurn>().distance = crate::combat::DuelDistance::FAR;
        app.update();

        let quick = find_button(&mut app, "quick-strike");
        let mark = find_reach_mark(&mut app, quick);
        assert_eq!(
            app.world().get::<Visibility>(mark),
            Some(&Visibility::Inherited),
            "an out-of-reach strike shows its distance mark"
        );
        assert_eq!(
            find_cost_or_reason_text(&mut app, quick),
            "Prea departe pentru lovitură."
        );

        // Non-strike rows never show the mark, even out of reach.
        let rest = find_button(&mut app, "rest");
        let rest_mark = find_reach_mark(&mut app, rest);
        assert_eq!(
            app.world().get::<Visibility>(rest_mark),
            Some(&Visibility::Hidden)
        );

        // Back in reach, the mark hides again.
        app.world_mut().resource_mut::<CombatTurn>().distance = crate::combat::DuelDistance::CLOSE;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(mark),
            Some(&Visibility::Hidden),
            "the mark hides once the strike is back in reach"
        );
    }

    /// The chip's current `(alpha, scale_x)`.
    fn chip_state(app: &mut App) -> (f32, f32) {
        let (color, transform) = app
            .world_mut()
            .query_filtered::<(&TextColor, &Transform), With<GroundDistanceChip>>()
            .single(app.world())
            .expect("one ground distance chip");
        (color.0.alpha(), transform.scale.x)
    }

    #[test]
    fn hovering_a_reach_disabled_strike_pulses_the_ground_distance_chip() {
        let mut app = test_app();
        app.world_mut().resource_mut::<CombatTurn>().distance = crate::combat::DuelDistance::FAR;
        app.update();
        assert_eq!(
            chip_state(&mut app),
            (crate::arena::GROUND_CHIP_ALPHA, 1.0),
            "unhovered, the chip rests at its etched baseline"
        );

        let quick = find_button(&mut app, "quick-strike");
        app.world_mut()
            .entity_mut(quick)
            .insert(Interaction::Hovered);
        app.update();
        let (alpha, scale) = chip_state(&mut app);
        assert!(
            alpha > crate::arena::GROUND_CHIP_ALPHA,
            "hovering the reach-disabled strike lifts the chip's alpha ({alpha})"
        );
        assert!(
            scale > 1.0,
            "hovering the reach-disabled strike scales the chip up subtly ({scale})"
        );

        app.world_mut().entity_mut(quick).insert(Interaction::None);
        app.update();
        assert_eq!(
            chip_state(&mut app),
            (crate::arena::GROUND_CHIP_ALPHA, 1.0),
            "unhovering restores the resting look exactly"
        );

        // Back in melee reach the strike is position-legal again: hovering
        // it must not pulse anything.
        app.world_mut().resource_mut::<CombatTurn>().distance = crate::combat::DuelDistance::CLOSE;
        app.world_mut()
            .entity_mut(quick)
            .insert(Interaction::Hovered);
        app.update();
        assert_eq!(
            chip_state(&mut app),
            (crate::arena::GROUND_CHIP_ALPHA, 1.0),
            "an in-reach strike's hover never pulses the chip"
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
        // Depth-first: the banner row nests its texts in a column beside
        // the pictogram tile, phone rows keep them as direct children.
        fn search(app: &App, entity: Entity) -> Option<String> {
            if app.world().get::<ActionCostOrReason>(entity).is_some() {
                return Some(
                    app.world()
                        .get::<Text>(entity)
                        .expect("cost/reason node has Text")
                        .0
                        .clone(),
                );
            }
            let children = app.world().get::<Children>(entity)?.to_vec();
            children.into_iter().find_map(|child| search(app, child))
        }
        search(app, button).unwrap_or_else(|| panic!("no ActionCostOrReason child found"))
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
            "normal-strike",
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
    fn a_test_registered_extra_descriptor_renders_and_emits_with_no_layout_edits() {
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
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
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
            buttons, 9,
            "eight real descriptors plus the test-registered ninth"
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
        // the eight real ones, with zero edits to this module's layout code.
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
    fn without_a_test_registration_the_palette_stays_at_eight_buttons() {
        let mut app = test_app();
        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(buttons, 8, "ExtraDescriptors defaults to empty");
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
                "the eight real actions span exactly four categories today"
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
                vec!["heavy-strike", "normal-strike", "quick-strike"],
                "only the Strikes category's three registered actions appear"
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
                vec!["heavy-strike", "normal-strike", "quick-strike"]
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
                definition: crate::character::CharacterDefinition::legacy_human(
                    crate::character::PlayerAppearance::default(),
                ),
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
            assert_eq!(action_button_ids(&mut app).len(), 8, "starts desktop-flat");

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
        fn crossing_back_to_desktop_at_runtime_restores_the_flat_eight_button_row() {
            let mut app = mobile_test_app();
            let strikes_button = find_category_button(&mut app, ActionCategory::Strikes);
            press_button(&mut app, strikes_button);
            assert_eq!(action_button_ids(&mut app).len(), 3, "opened on phone");

            app.world_mut()
                .resource_mut::<ViewportInfo>()
                .set_if_neq(ViewportInfo::default());
            app.update();

            assert_eq!(
                action_button_ids(&mut app).len(),
                8,
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

    // --- #276's mobile layout contract ---

    mod mobile_layout {
        use super::*;

        /// The real `LetterboxRect` a 390x844 phone viewport produces (see
        /// `core::letterbox_camera`): the fixed 4:3 design resolution
        /// (800x600) letterboxes to a 390x292.5 band, vertically centered --
        /// i.e. a large, otherwise-unused strip both above and below it.
        /// Hand-computed here (rather than driven through a real `App`,
        /// which never runs `letterbox_camera` headlessly -- it needs a real
        /// `Window` -- so `LetterboxRect` stays at its full-resolution
        /// default in every existing headless test app) so this module's
        /// tests exercise the exact numbers #276 was filed against.
        fn phone_letterbox() -> LetterboxRect {
            LetterboxRect {
                position: Vec2::new(0.0, 275.75),
                size: Vec2::new(390.0, 292.5),
            }
        }

        fn phone_viewport() -> ViewportInfo {
            ViewportInfo {
                width: 390.0,
                height: 844.0,
                is_mobile: true,
            }
        }

        /// The phone action bar's tallest possible height: both rows open
        /// (a category row plus a populated action row), the gap between
        /// them, and the container's own padding on both the top and bottom
        /// edge.
        fn max_phone_bar_height() -> f32 {
            PHONE_TARGET_HEIGHT * 2.0 + PHONE_ROW_GAP + ACTION_BAR_PADDING * 2.0
        }

        #[test]
        fn reserves_the_unused_strip_below_a_tall_phones_letterboxed_stage() {
            let offset = phone_bar_bottom_offset(phone_viewport(), phone_letterbox());
            // stage_bottom = 275.75 + 292.5 = 568.25; below_stage_room =
            // 844.0 - 568.25 = 275.75; offset = 8.0 - 275.75.
            assert_eq!(
                offset,
                8.0 - 275.75,
                "must anchor against the real window's bottom edge, not the stage's"
            );
        }

        #[test]
        fn falls_back_to_the_flat_margin_with_no_letterbox_slack() {
            // A "mobile-width" viewport whose letterboxed stage already
            // spans the full window height (no unused strip below it to
            // reserve) -- e.g. a squarish or landscape-shaped window, which
            // `is_mobile_width`'s width-only breakpoint does not itself rule
            // out.
            let viewport = ViewportInfo {
                width: 690.0,
                height: 517.5,
                is_mobile: true,
            };
            let letterbox = LetterboxRect {
                position: Vec2::ZERO,
                size: Vec2::new(690.0, 517.5),
            };
            let offset = phone_bar_bottom_offset(viewport, letterbox);
            assert_eq!(
                offset, PHONE_BAR_WINDOW_MARGIN,
                "with no slack below the stage, must keep the pre-#276 flat margin"
            );
        }

        /// The #276 acceptance criterion, proven geometrically: on a tall
        /// phone (real letterbox slack below the stage), even the tallest
        /// possible bar -- both rows open -- must land entirely below the
        /// stage's own bottom edge, so it can never intersect anything
        /// inside the stage (the fighters or the combat log, both confined
        /// there like every full-window HUD overlay per #125).
        #[test]
        fn the_tallest_open_bar_never_reaches_back_into_the_stage() {
            let letterbox = phone_letterbox();
            let offset = phone_bar_bottom_offset(phone_viewport(), letterbox);
            // The container's bottom edge sits `-offset` px below the
            // stage's own bottom edge (`offset` is negative here); its top
            // edge is `max_phone_bar_height()` above that. For the whole box
            // to stay outside the stage, that top edge must still be at or
            // below the stage's bottom edge -- i.e. `-offset` must be at
            // least as large as the tallest possible bar.
            assert!(
                -offset >= max_phone_bar_height(),
                "the fully-open bar (height {}) must fit entirely within the {} px reserved \
                 below the stage (offset {offset})",
                max_phone_bar_height(),
                -offset
            );
        }

        /// Integration proof that [`spawn_phone_action_bar`] actually wires
        /// the contract through: the spawned container's declared
        /// `Node.bottom` must equal what [`phone_bar_bottom_offset`] computes
        /// from the exact same `ViewportInfo`/`LetterboxRect` resources the
        /// app spawned it with (whatever they happen to be headlessly --
        /// see `phone_letterbox`'s doc comment for why a headless app's
        /// `LetterboxRect` is not the real 390x844 value), not a value
        /// independently derived or left over from the pre-#276 flat
        /// constant.
        #[test]
        fn the_spawned_phone_bar_uses_the_contracts_bottom_offset() {
            let mut app = mobile_test_app();
            let viewport = *app.world().resource::<ViewportInfo>();
            let letterbox = *app.world().resource::<LetterboxRect>();
            let expected = phone_bar_bottom_offset(viewport, letterbox);

            let node = app
                .world_mut()
                .query_filtered::<&Node, With<ActionBarRoot>>()
                .single(app.world())
                .expect("one action bar root");
            assert_eq!(
                node.bottom,
                Val::Px(expected),
                "the spawned phone bar's bottom offset must come from phone_bar_bottom_offset"
            );
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
        fn desktop_tab_order_matches_the_eight_visible_buttons_left_to_right() {
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
                "normal-strike",
                "heavy-strike",
                "step-forward",
                "leap-forward",
                "step-back",
                "block",
                "rest",
            ];
            let mut seen = Vec::new();
            for _ in 0..expected.len() {
                press_key_and_settle(&mut app, KeyCode::Tab);
                seen.push(focused_action_id(&mut app).expect("a button is focused"));
            }
            assert_eq!(
                seen, expected,
                "tab order follows the banner's group-by-group decision order"
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

            let expected_actions = ["quick-strike", "normal-strike", "heavy-strike"];
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
                vec!["heavy-strike", "normal-strike", "quick-strike"],
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
                definition: crate::character::CharacterDefinition::legacy_human(
                    crate::character::PlayerAppearance::default(),
                ),
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
            for _ in 0..10 {
                press_key_and_settle(&mut app, KeyCode::Tab);
            }
            let visited: Vec<ActionId> = (0..10)
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
