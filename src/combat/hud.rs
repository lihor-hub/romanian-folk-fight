//! Combat HUD for the fight screen: per-fighter health/stamina panels, the
//! four action buttons, and the scrolling combat log.
//!
//! The pure pieces (bar percentages, the button-enabled predicate, and the
//! event → log-line formatting) are plain functions so they stay
//! unit-testable and reusable — the announcer issue builds its flavor text on
//! top of the same [`CombatLogEvent`] stream, so [`log_line`] keeps to plain
//! factual wording.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::character::{EnemyFighter, FighterName, Health, PlayerFighter, Stamina};
use crate::core::{UiFont, ViewportInfo};
use crate::menu::DisabledButton;
use crate::progression::Level;
use crate::theme::{
    ACTION_BUTTON_TOUCH_TARGET, BAR_TRACK, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL,
    BUTTON_PRESSED, CREAM, GOLD, HP_FILL, MOBILE_LOG_LINES, PANEL_LINEN, PanelTexture,
    STAMINA_FILL, TEXT_DISABLED, WALNUT, panel_bundle,
};

use super::engine::{CombatAction, CombatEvent, DuelDistance, REST_RESTORE};
use super::systems::{
    CombatLogEvent, CombatPresentation, CombatSide, CombatTurn, PlayerActionEvent,
};

/// How many log lines the combat log keeps and shows.
pub const LOG_CAPACITY: usize = 8;

const PANEL_WIDTH: f32 = 240.0;
const BAR_HEIGHT: f32 = 16.0;
#[cfg(test)]
const HUD_TARGET_WIDTH: f32 = 800.0;
#[cfg(test)]
const ACTION_BUTTON_COUNT: f32 = 7.0;
const ACTION_BUTTON_WIDTH: f32 = 100.0;
const ACTION_BUTTON_HEIGHT: f32 = 64.0;
const ACTION_BAR_DESKTOP_GAP: f32 = 6.0;
const ACTION_BAR_PADDING: f32 = 8.0;
const ACTION_BAR_DESKTOP_INSET: f32 = 10.0;

/// Marker for the HUD root; everything under it despawns on
/// `OnExit(GameState::Fight)`.
#[derive(Component)]
pub(super) struct HudScreen;

/// One of the two pools a bar or label can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pool {
    Health,
    Stamina,
}

/// The colored fill node of a bar; its width is driven every frame from the
/// owning fighter's pool.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BarFill {
    side: CombatSide,
    pool: Pool,
}

/// A text label refreshed from fighter components every frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HudLabel {
    /// The fighter's display name.
    Name(CombatSide),
    /// A `current/max` readout next to the matching bar.
    Pool { side: CombatSide, pool: Pool },
}

/// Marker for the combat-log text node.
#[derive(Component)]
pub(super) struct LogText;

/// Marker for one side's status panel, so the responsive layout system (#31)
/// can shrink it under the mobile breakpoint.
#[derive(Component)]
pub(super) struct FighterPanelRoot;

/// Marker for the action-button row, so it can switch to a 2×2 grid under
/// the mobile breakpoint.
#[derive(Component)]
pub(super) struct ActionBarRoot;

/// Marker for the combat-log panel, repositioned/resized under the mobile
/// breakpoint so it doesn't overlap the taller 2×2 action grid.
#[derive(Component)]
pub(super) struct LogPanelRoot;

/// The combat action a HUD button submits when clicked.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionButton(CombatAction);

/// Small glyph at the head of an action tile; stable so tests can confirm
/// buttons are icon-led without depending on screenshots.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionGlyph(CombatAction);

/// The last [`LOG_CAPACITY`] combat-log lines, oldest first. Lives only while
/// the fight screen is up.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct CombatLog {
    lines: VecDeque<String>,
}

impl CombatLog {
    /// Appends a line, dropping the oldest beyond [`LOG_CAPACITY`].
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push_back(line.into());
        while self.lines.len() > LOG_CAPACITY {
            self.lines.pop_front();
        }
    }

    /// The kept lines, oldest first (newest belongs at the bottom).
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }

    /// All kept lines joined for a single text node, newest at the bottom.
    pub fn to_text(&self) -> String {
        self.lines().collect::<Vec<_>>().join("\n")
    }

    /// Like [`Self::to_text`], but only the last `max_lines` (or all of them,
    /// if fewer) — the mobile HUD (#31) collapses the log to the last
    /// [`crate::theme::MOBILE_LOG_LINES`] lines to save vertical space.
    pub fn to_text_capped(&self, max_lines: usize) -> String {
        let skip = self.lines.len().saturating_sub(max_lines);
        self.lines().skip(skip).collect::<Vec<_>>().join("\n")
    }
}

/// The fill width in percent for a `current/max` pool, clamped to `[0, 100]`
/// so an emptied or overfilled pool never renders outside the track.
pub fn bar_percent(current: i32, max: i32) -> f32 {
    if max <= 0 {
        return 0.0;
    }
    (100.0 * current as f32 / max as f32).clamp(0.0, 100.0)
}

/// The Romanian button label for an action.
pub fn action_label(action: CombatAction) -> &'static str {
    match action {
        CombatAction::QuickStrike => "Lovitură iute",
        CombatAction::HeavyStrike => "Lovitură grea",
        CombatAction::Block => "Apărare",
        CombatAction::Rest => "Odihnă",
        CombatAction::StepForward => "Pas înainte",
        CombatAction::StepBack => "Pas înapoi",
        CombatAction::LeapForward => "Salt înainte",
    }
}

/// The stamina-cost line under a button label. Rest costs nothing (it
/// restores [`REST_RESTORE`] instead).
pub fn cost_label(action: CombatAction) -> String {
    match action {
        CombatAction::Rest => format!("+{REST_RESTORE} stamina"),
        CombatAction::StepForward | CombatAction::StepBack | CombatAction::LeapForward => {
            "poziție".to_string()
        }
        _ => format!("-{} stamina", action.stamina_cost()),
    }
}

/// Whether an action button is clickable, matching the engine's rules
/// exactly: it must be the player's turn in a running duel, and strikes are
/// rejected below their stamina cost. Block and Rest never require stamina
/// ([`super::engine::resolve_action`] saturates the block cost at zero).
pub fn action_enabled(
    turn: &CombatTurn,
    stamina: i32,
    presentation_busy: bool,
    action: CombatAction,
) -> bool {
    if turn.side != CombatSide::Player || turn.over || presentation_busy {
        return false;
    }
    match action {
        CombatAction::QuickStrike | CombatAction::HeavyStrike => {
            turn.distance.in_melee_reach() && stamina >= action.stamina_cost()
        }
        CombatAction::Block | CombatAction::Rest => true,
        CombatAction::StepForward => turn.distance.band() > DuelDistance::CLOSE.band(),
        CombatAction::StepBack => turn.distance.band() < DuelDistance::FAR.band(),
        CombatAction::LeapForward => turn.distance.band() > DuelDistance::CLOSE.band(),
    }
}

/// Formats one [`CombatEvent`] as a plain factual log line. `actor` performed
/// the action; `opponent` is the other fighter (the one blocking or defeated).
pub fn log_line(actor: &str, opponent: &str, event: CombatEvent) -> String {
    match event {
        CombatEvent::Missed => format!("{actor} ratează lovitura."),
        CombatEvent::OutOfReach => format!("{actor} este prea departe pentru lovitură."),
        CombatEvent::Hit { dmg } => format!("{actor} lovește pentru {dmg}!"),
        CombatEvent::Crit { dmg } => format!("{actor} dă o lovitură critică pentru {dmg}!"),
        CombatEvent::Blocked { dmg } => format!("{opponent} blochează: doar {dmg} daune."),
        CombatEvent::Guarded => format!("{actor} ridică garda."),
        CombatEvent::Rested { amount } => {
            format!("{actor} se odihnește și recuperează {amount} stamina.")
        }
        CombatEvent::Moved { from, to } if from == to => {
            format!("{actor} își ține poziția.")
        }
        CombatEvent::Moved { from, to } if to.band() < from.band() => {
            format!("{actor} înaintează în arenă.")
        }
        CombatEvent::Moved { .. } => format!("{actor} se retrage un pas."),
        CombatEvent::OutOfStamina => format!("{actor} nu are destulă stamina!"),
        CombatEvent::Defeated => format!("{opponent} este învins!"),
    }
}

/// Spawns the HUD overlay and a fresh [`CombatLog`] on entering the fight.
pub(super) fn spawn_hud(
    mut commands: Commands,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    viewport: Res<ViewportInfo>,
) {
    commands.insert_resource(CombatLog::default());
    let is_mobile = viewport.is_mobile;
    commands.spawn((
        HudScreen,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        children![
            fighter_panel(CombatSide::Player, &ui_font, &panel_texture, is_mobile),
            fighter_panel(CombatSide::Enemy, &ui_font, &panel_texture, is_mobile),
            pause_button(&ui_font),
            log_panel(&ui_font, &panel_texture, is_mobile),
            action_bar(&ui_font, &panel_texture, is_mobile),
        ],
    ));
}

/// The small, touch-friendly ⏸ button top-center of the HUD; clicking it
/// opens the pause overlay (see [`super::pause`]).
fn pause_button(ui_font: &UiFont) -> impl Bundle {
    (
        Button,
        super::pause::PauseButton,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Percent(50.0),
            // Center the fixed-width button on the 50% anchor.
            margin: UiRect::left(Val::Px(-24.0)),
            width: Val::Px(48.0),
            height: Val::Px(48.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BUTTON_NORMAL),
        children![(
            // "||" instead of "⏸": U+23F8 has no glyph in the bundled font.
            Text::new("||"),
            ui_font.text_font(24.0),
            TextColor(CREAM),
        )],
    )
}

/// Drops the combat log on leaving the fight; the HUD entities are removed by
/// `despawn_screen::<HudScreen>`.
pub(super) fn teardown_hud(mut commands: Commands) {
    commands.remove_resource::<CombatLog>();
}

/// Narrower fighter-panel width under the mobile breakpoint, so both panels
/// fit side by side above a portrait-phone viewport without overlapping.
const PANEL_WIDTH_MOBILE: f32 = 150.0;

/// One fighter's status panel in a top corner: name, HP bar, stamina bar.
fn fighter_panel(
    side: CombatSide,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
) -> impl Bundle {
    let width = if is_mobile {
        PANEL_WIDTH_MOBILE
    } else {
        PANEL_WIDTH
    };
    let mut node = Node {
        position_type: PositionType::Absolute,
        top: Val::Px(12.0),
        width: Val::Px(width),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        padding: UiRect::all(Val::Px(10.0)),
        ..default()
    };
    match side {
        CombatSide::Player => node.left = Val::Px(12.0),
        CombatSide::Enemy => node.right = Val::Px(12.0),
    }
    (
        panel_bundle(panel_texture, node),
        FighterPanelRoot,
        children![
            (
                Text::new(""),
                ui_font.text_font(20.0),
                TextColor(CREAM),
                HudLabel::Name(side),
            ),
            bar(side, Pool::Health, HP_FILL, ui_font),
            bar(side, Pool::Stamina, STAMINA_FILL, ui_font),
        ],
    )
}

/// A thin gold edge, per the palette, drawn on carved-wood bar tracks.
const BAR_EDGE: Color = crate::theme::GOLD;

/// One bar row: a carved-wood track with a thin gold edge and a colored
/// fill, plus a `current/max` label.
fn bar(side: CombatSide, pool: Pool, fill_color: Color, ui_font: &UiFont) -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        },
        children![
            (
                Node {
                    flex_grow: 1.0,
                    height: Val::Px(BAR_HEIGHT),
                    border: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BackgroundColor(BAR_TRACK),
                BorderColor::all(BAR_EDGE),
                children![(
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(fill_color),
                    BarFill { side, pool },
                )],
            ),
            (
                Text::new(""),
                ui_font.text_font(14.0),
                TextColor(CREAM),
                HudLabel::Pool { side, pool },
            ),
        ],
    )
}

/// The right-side combat-log panel with a single multi-line text node. Under
/// the mobile breakpoint it moves up and narrows so it clears the taller 2×2
/// action grid at the bottom of the screen.
fn log_panel(ui_font: &UiFont, panel_texture: &PanelTexture, is_mobile: bool) -> impl Bundle {
    let node = if is_mobile {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            right: Val::Px(12.0),
            top: Val::Px(96.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        }
    } else {
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(120.0),
            width: Val::Px(300.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        }
    };
    (
        panel_bundle(panel_texture, node),
        LogPanelRoot,
        children![(
            Text::new(""),
            ui_font.text_font(15.0),
            TextColor(CREAM),
            LogText,
        )],
    )
}

/// The bottom action bar with combat and movement buttons: a single
/// (wrapping) row on desktop, a tighter wrap grid of ≥48px touch targets
/// under the mobile breakpoint (#31).
fn action_bar(ui_font: &UiFont, panel_texture: &PanelTexture, is_mobile: bool) -> impl Bundle {
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
    (
        panel_bundle(panel_texture, node),
        BackgroundColor(PANEL_LINEN),
        ActionBarRoot,
        children![
            action_button(CombatAction::QuickStrike, ui_font, is_mobile),
            action_button(CombatAction::HeavyStrike, ui_font, is_mobile),
            action_button(CombatAction::Block, ui_font, is_mobile),
            action_button(CombatAction::Rest, ui_font, is_mobile),
            action_button(CombatAction::StepForward, ui_font, is_mobile),
            action_button(CombatAction::StepBack, ui_font, is_mobile),
            action_button(CombatAction::LeapForward, ui_font, is_mobile),
        ],
    )
}

/// One action button: the Romanian label over its stamina cost. Under the
/// mobile breakpoint it grows to a ≥[`ACTION_BUTTON_TOUCH_TARGET`] square-ish
/// tile so four of them wrap into a 2×2 grid.
fn action_button(action: CombatAction, ui_font: &UiFont, is_mobile: bool) -> impl Bundle {
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
    (
        Button,
        ActionButton(action),
        node,
        BackgroundColor(BUTTON_NORMAL),
        children![
            (
                Node {
                    width: Val::Px(34.0),
                    height: Val::Px(20.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(WALNUT),
                BorderColor::all(GOLD),
                children![(
                    Text::new(action_glyph(action)),
                    ui_font.text_font_bold(14.0),
                    TextColor(GOLD),
                    ActionGlyph(action),
                )],
            ),
            (
                Text::new(action_label(action)),
                ui_font.text_font(15.0),
                TextColor(CREAM),
            ),
            (
                Text::new(cost_label(action)),
                ui_font.text_font(11.0),
                TextColor(CREAM),
            ),
        ],
    )
}

fn action_glyph(action: CombatAction) -> &'static str {
    match action {
        CombatAction::QuickStrike => ">>",
        CombatAction::HeavyStrike => "**",
        CombatAction::Block => "[]",
        CombatAction::Rest => "++",
        CombatAction::StepForward => "->",
        CombatAction::StepBack => "<-",
        CombatAction::LeapForward => "^>",
    }
}

#[cfg(test)]
fn desktop_action_strip_occupied_width() -> f32 {
    ACTION_BUTTON_WIDTH * ACTION_BUTTON_COUNT
        + ACTION_BAR_DESKTOP_GAP * (ACTION_BUTTON_COUNT - 1.0)
        + ACTION_BAR_PADDING * 2.0
}

#[cfg(test)]
fn desktop_action_strip_available_width() -> f32 {
    HUD_TARGET_WIDTH - ACTION_BAR_DESKTOP_INSET * 2.0
}

/// Query for the display data of one side's fighter.
type SideData<'w, 's, Side, OtherSide> = Query<
    'w,
    's,
    (&'static FighterName, &'static Health, &'static Stamina),
    (With<Side>, Without<OtherSide>),
>;
type PlayerData<'w, 's> = SideData<'w, 's, PlayerFighter, EnemyFighter>;
type EnemyData<'w, 's> = SideData<'w, 's, EnemyFighter, PlayerFighter>;

/// The display data of `side`, if that fighter exists.
fn side_data<'a>(
    side: CombatSide,
    player: &'a PlayerData,
    enemy: &'a EnemyData,
) -> Option<(&'a FighterName, &'a Health, &'a Stamina)> {
    match side {
        CombatSide::Player => player.single().ok(),
        CombatSide::Enemy => enemy.single().ok(),
    }
}

/// The `(current, max)` of one pool.
fn pool_values(pool: Pool, health: &Health, stamina: &Stamina) -> (i32, i32) {
    match pool {
        Pool::Health => (health.current, health.max),
        Pool::Stamina => (stamina.current, stamina.max),
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

/// Emits the clicked button's action as a [`PlayerActionEvent`] — the same
/// message the debug keyboard mapping writes. Disabled buttons are filtered
/// out entirely.
pub(super) fn handle_action_buttons(
    interactions: Query<(&Interaction, &ActionButton), ChangedEnabledButton>,
    mut actions: MessageWriter<PlayerActionEvent>,
) {
    for (interaction, ActionButton(action)) in &interactions {
        if *interaction == Interaction::Pressed {
            actions.write(PlayerActionEvent(*action));
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

/// Query data for [`update_action_buttons`]: a button, its action, whether it
/// is currently disabled, and what it needs restyled.
type AvailabilityControlled = (
    Entity,
    &'static ActionButton,
    Has<DisabledButton>,
    &'static mut BackgroundColor,
    &'static Children,
);

/// Greys out (and un-greys) action buttons to match [`action_enabled`]. Only
/// touches buttons whose enabled state actually flipped, so it does not fight
/// the hover-feedback system.
pub(super) fn update_action_buttons(
    mut commands: Commands,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    player: Query<&Stamina, With<PlayerFighter>>,
    mut buttons: Query<AvailabilityControlled, With<Button>>,
    mut text_colors: Query<&mut TextColor>,
) {
    let stamina = player.single().map(|s| s.current).unwrap_or(0);
    let presentation_busy = presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy);
    for (entity, ActionButton(action), was_disabled, mut background, children) in &mut buttons {
        let enabled = turn
            .as_deref()
            .is_some_and(|turn| action_enabled(turn, stamina, presentation_busy, *action));
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
            if let Ok(mut color) = text_colors.get_mut(child) {
                color.0 = text_color;
            }
        }
    }
}

/// Drives every bar fill's width from the owning fighter's pool.
pub(super) fn update_bar_fills(
    player: PlayerData,
    enemy: EnemyData,
    mut fills: Query<(&BarFill, &mut Node)>,
) {
    for (fill, mut node) in &mut fills {
        let Some((_, health, stamina)) = side_data(fill.side, &player, &enemy) else {
            continue;
        };
        let (current, max) = pool_values(fill.pool, health, stamina);
        let width = Val::Percent(bar_percent(current, max));
        if node.width != width {
            node.width = width;
        }
    }
}

/// Query filter: any fighter datum a HUD label displays changed this frame.
type FighterDataChanged = Or<(Changed<FighterName>, Changed<Health>, Changed<Stamina>)>;

/// Refreshes the name and `current/max` labels from the fighter components.
/// Skips frames where no fighter data changed, so the string formatting only
/// runs when a label could actually differ.
pub(super) fn update_labels(
    player: PlayerData,
    enemy: EnemyData,
    level: Option<Res<Level>>,
    changed: Query<(), FighterDataChanged>,
    mut labels: Query<(&HudLabel, &mut Text)>,
) {
    if changed.is_empty() {
        return;
    }
    for (label, mut text) in &mut labels {
        let side = match label {
            HudLabel::Name(side) | HudLabel::Pool { side, .. } => *side,
        };
        let Some((name, health, stamina)) = side_data(side, &player, &enemy) else {
            continue;
        };
        let value = match label {
            HudLabel::Name(_) => match level.as_deref() {
                // The player's panel carries their level; enemy levels come
                // with the roster issue.
                Some(level) if side == CombatSide::Player => {
                    format!("{} (Nv. {})", name.0, level.level)
                }
                _ => name.0.clone(),
            },
            HudLabel::Pool { pool, .. } => {
                let (current, max) = pool_values(*pool, health, stamina);
                format!("{current}/{max}")
            }
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Turns this frame's [`CombatLogEvent`]s into log lines on the [`CombatLog`].
pub(super) fn collect_log_lines(
    mut events: MessageReader<CombatLogEvent>,
    log: Option<ResMut<CombatLog>>,
    player: Query<&FighterName, (With<PlayerFighter>, Without<EnemyFighter>)>,
    enemy: Query<&FighterName, (With<EnemyFighter>, Without<PlayerFighter>)>,
) {
    let Some(mut log) = log else {
        return;
    };
    let player_name = player.single().map(|n| n.0.as_str()).unwrap_or("?");
    let enemy_name = enemy.single().map(|n| n.0.as_str()).unwrap_or("?");
    for CombatLogEvent { actor, event, .. } in events.read().copied() {
        let (actor_name, opponent_name) = match actor {
            CombatSide::Player => (player_name, enemy_name),
            CombatSide::Enemy => (enemy_name, player_name),
        };
        log.push(log_line(actor_name, opponent_name, event));
    }
}

/// Rewrites the log text node whenever the [`CombatLog`] changed, capping it
/// to the last [`MOBILE_LOG_LINES`] under the mobile breakpoint (#31).
pub(super) fn update_log_text(
    log: Res<CombatLog>,
    viewport: Res<ViewportInfo>,
    mut texts: Query<&mut Text, With<LogText>>,
) {
    for mut text in &mut texts {
        let value = if viewport.is_mobile {
            log.to_text_capped(MOBILE_LOG_LINES)
        } else {
            log.to_text()
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Query filter for one responsive-layout node kind: it carries `Root` but
/// none of the other three root markers, so the four queries in
/// [`apply_responsive_hud_layout`] never alias the same entity.
type ResponsiveNodeFilter<Root, A, B, C> = (With<Root>, Without<A>, Without<B>, Without<C>);

/// Re-flows the HUD when [`ViewportInfo`] crosses the mobile breakpoint:
/// resizes the fighter panels, the action grid/buttons, and the log panel in
/// place instead of respawning the whole HUD.
pub(super) fn apply_responsive_hud_layout(
    viewport: Res<ViewportInfo>,
    mut panels: Query<
        &mut Node,
        ResponsiveNodeFilter<FighterPanelRoot, ActionBarRoot, LogPanelRoot, ActionButton>,
    >,
    mut action_bars: Query<
        &mut Node,
        ResponsiveNodeFilter<ActionBarRoot, FighterPanelRoot, LogPanelRoot, ActionButton>,
    >,
    mut buttons: Query<
        &mut Node,
        ResponsiveNodeFilter<ActionButton, FighterPanelRoot, ActionBarRoot, LogPanelRoot>,
    >,
    mut logs: Query<
        &mut Node,
        ResponsiveNodeFilter<LogPanelRoot, FighterPanelRoot, ActionBarRoot, ActionButton>,
    >,
) {
    if !viewport.is_changed() {
        return;
    }
    let is_mobile = viewport.is_mobile;
    let panel_width = if is_mobile {
        PANEL_WIDTH_MOBILE
    } else {
        PANEL_WIDTH
    };
    for mut node in &mut panels {
        node.width = Val::Px(panel_width);
    }
    for mut node in &mut action_bars {
        node.flex_wrap = if is_mobile {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        };
        node.left = Val::Px(if is_mobile { 8.0 } else { 0.0 });
        node.right = Val::Px(if is_mobile { 8.0 } else { 0.0 });
        node.column_gap = Val::Px(if is_mobile {
            8.0
        } else {
            ACTION_BAR_DESKTOP_GAP
        });
        node.row_gap = Val::Px(if is_mobile { 8.0 } else { 0.0 });
    }
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
    for mut node in &mut logs {
        if is_mobile {
            node.left = Val::Px(12.0);
            node.right = Val::Px(12.0);
            node.top = Val::Px(96.0);
            node.width = Val::Auto;
        } else {
            node.left = Val::Auto;
            node.right = Val::Px(12.0);
            node.top = Val::Px(120.0);
            node.width = Val::Px(300.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::systems::{CombatPlugin, CombatRng};
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::Attributes;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MIN};
    use crate::core::{CorePlugin, GameState};
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use bevy::state::app::StatesPlugin;
    use rand::{RngExt as _, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use std::time::Duration;

    /// Same player build as the combat systems tests: putere 4 (damage 6),
    /// agilitate 2 (ties the Hoț de codru), vitalitate 4 (90 hp, 50 stamina).
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    const PLAYER_TURN: CombatTurn = CombatTurn {
        side: CombatSide::Player,
        over: false,
        player_blocking: false,
        enemy_blocking: false,
        distance: DuelDistance::CLOSE,
    };

    /// Headless app on the fight screen with a deterministic duel RNG whose
    /// first four strikes are clean hits without crits.
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

    /// A `ChaCha8Rng` whose first `strikes` strikes are guaranteed clean
    /// hits without crits (same construction as the combat systems tests).
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

    fn find_button(app: &mut App, action: CombatAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &ActionButton), With<Button>>()
            .iter(app.world())
            .find(|(_, a)| a.0 == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button for {action:?} exists"))
    }

    /// Clicks a HUD button the way a mouse does: a `Pressed` interaction.
    fn press_button(app: &mut App, action: CombatAction) {
        let button = find_button(app, action);
        app.world_mut()
            .entity_mut(button)
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
        press_button(app, action);
        advance_presentation(app);
        advance_presentation(app);
    }

    fn turn(app: &App) -> CombatTurn {
        *app.world().resource::<CombatTurn>()
    }

    fn pools<M: Component>(app: &mut App) -> (i32, i32) {
        let (health, stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<M>>()
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

    /// Forces the AI's deterministic Rest branch so player-side expectations
    /// stay exact (same trick as the combat systems tests).
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

    fn set_enemy_health(app: &mut App, value: i32) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Health, With<EnemyFighter>>();
        query
            .single_mut(app.world_mut())
            .expect("enemy fighter exists")
            .current = value;
    }

    fn fill_width(app: &mut App, side: CombatSide, pool: Pool) -> Val {
        app.world_mut()
            .query::<(&BarFill, &Node)>()
            .iter(app.world())
            .find(|(fill, _)| fill.side == side && fill.pool == pool)
            .map(|(_, node)| node.width)
            .expect("bar fill exists")
    }

    fn label_text(app: &mut App, wanted: HudLabel) -> String {
        app.world_mut()
            .query::<(&HudLabel, &Text)>()
            .iter(app.world())
            .find(|(label, _)| **label == wanted)
            .map(|(_, text)| text.0.clone())
            .unwrap_or_else(|| panic!("label {wanted:?} exists"))
    }

    fn log_text(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Text, With<LogText>>()
            .single(app.world())
            .expect("log text exists")
            .0
            .clone()
    }

    // --- pure pieces ---

    #[test]
    fn bar_percent_scales_and_clamps_to_the_track() {
        assert_eq!(bar_percent(0, 100), 0.0, "0 hp renders empty");
        assert_eq!(bar_percent(50, 100), 50.0);
        assert_eq!(bar_percent(100, 100), 100.0);
        assert_eq!(bar_percent(140, 70), 100.0, "overheal clamps to full");
        assert_eq!(bar_percent(-5, 100), 0.0, "never below the track");
        assert_eq!(bar_percent(10, 0), 0.0, "degenerate max renders empty");
    }

    #[test]
    fn log_line_formats_every_event_kind() {
        let cases = [
            (CombatEvent::Missed, "Făt-Frumos ratează lovitura."),
            (
                CombatEvent::Hit { dmg: 12 },
                "Făt-Frumos lovește pentru 12!",
            ),
            (
                CombatEvent::Crit { dmg: 24 },
                "Făt-Frumos dă o lovitură critică pentru 24!",
            ),
            (
                CombatEvent::Blocked { dmg: 3 },
                "Strigoi blochează: doar 3 daune.",
            ),
            (CombatEvent::Guarded, "Făt-Frumos ridică garda."),
            (
                CombatEvent::Rested { amount: 20 },
                "Făt-Frumos se odihnește și recuperează 20 stamina.",
            ),
            (
                CombatEvent::OutOfReach,
                "Făt-Frumos este prea departe pentru lovitură.",
            ),
            (
                CombatEvent::Moved {
                    from: DuelDistance::FAR,
                    to: DuelDistance::NEAR,
                },
                "Făt-Frumos înaintează în arenă.",
            ),
            (
                CombatEvent::Moved {
                    from: DuelDistance::NEAR,
                    to: DuelDistance::FAR,
                },
                "Făt-Frumos se retrage un pas.",
            ),
            (
                CombatEvent::OutOfStamina,
                "Făt-Frumos nu are destulă stamina!",
            ),
            (CombatEvent::Defeated, "Strigoi este învins!"),
        ];
        for (event, expected) in cases {
            assert_eq!(log_line("Făt-Frumos", "Strigoi", event), expected);
        }
    }

    #[test]
    fn combat_log_keeps_only_the_last_eight_lines_in_order() {
        let mut log = CombatLog::default();
        for i in 1..=10 {
            log.push(format!("line {i}"));
        }
        let lines: Vec<&str> = log.lines().collect();
        let expected: Vec<String> = (3..=10).map(|i| format!("line {i}")).collect();
        assert_eq!(lines, expected, "oldest two lines dropped");
        assert_eq!(log.to_text(), expected.join("\n"), "newest at the bottom");
    }

    #[test]
    fn to_text_capped_keeps_only_the_newest_lines() {
        let mut log = CombatLog::default();
        for i in 1..=8 {
            log.push(format!("line {i}"));
        }
        // Mobile HUD (#31): the last MOBILE_LOG_LINES only.
        assert_eq!(
            log.to_text_capped(crate::theme::MOBILE_LOG_LINES),
            "line 6\nline 7\nline 8"
        );
        // A cap bigger than the stored history returns everything.
        assert_eq!(log.to_text_capped(100), log.to_text());
    }

    #[test]
    fn action_enabled_matches_the_engine_rules() {
        use CombatAction::*;
        let enemy_turn = CombatTurn {
            side: CombatSide::Enemy,
            ..PLAYER_TURN
        };
        let over = CombatTurn {
            over: true,
            ..PLAYER_TURN
        };
        let far = CombatTurn {
            distance: DuelDistance::FAR,
            ..PLAYER_TURN
        };
        let cases = [
            // (turn, stamina, action, expected, why)
            (PLAYER_TURN, 50, QuickStrike, true, "affordable on my turn"),
            (enemy_turn, 50, QuickStrike, false, "not my turn"),
            (over, 50, QuickStrike, false, "duel is over"),
            (far, 50, QuickStrike, false, "too far for quick strike"),
            (PLAYER_TURN, 4, QuickStrike, false, "below the 5 cost"),
            (PLAYER_TURN, 5, QuickStrike, true, "exactly the 5 cost"),
            (far, 50, HeavyStrike, false, "too far for heavy strike"),
            (PLAYER_TURN, 14, HeavyStrike, false, "below the 15 cost"),
            (PLAYER_TURN, 15, HeavyStrike, true, "exactly the 15 cost"),
            (PLAYER_TURN, 0, Block, true, "block never rejects"),
            (PLAYER_TURN, 0, Rest, true, "rest never rejects"),
            (PLAYER_TURN, 0, StepForward, false, "already close"),
            (PLAYER_TURN, 0, StepBack, true, "can open distance"),
            (far, 0, StepForward, true, "can close distance"),
            (far, 0, StepBack, false, "already at max distance"),
            (far, 0, LeapForward, true, "can leap from range"),
            (over, 0, Rest, false, "nothing after the duel ends"),
        ];
        for (turn, stamina, action, expected, why) in cases {
            assert_eq!(
                action_enabled(&turn, stamina, false, action),
                expected,
                "{why}"
            );
        }
        assert!(
            !action_enabled(&PLAYER_TURN, 50, true, QuickStrike),
            "presentation busy disables otherwise-valid actions"
        );
    }

    #[test]
    fn buttons_carry_romanian_labels_and_stamina_costs() {
        assert_eq!(action_label(CombatAction::QuickStrike), "Lovitură iute");
        assert_eq!(action_label(CombatAction::HeavyStrike), "Lovitură grea");
        assert_eq!(action_label(CombatAction::Block), "Apărare");
        assert_eq!(action_label(CombatAction::Rest), "Odihnă");
        assert_eq!(action_label(CombatAction::StepForward), "Pas înainte");
        assert_eq!(action_label(CombatAction::StepBack), "Pas înapoi");
        assert_eq!(action_label(CombatAction::LeapForward), "Salt înainte");
        assert_eq!(cost_label(CombatAction::QuickStrike), "-5 stamina");
        assert_eq!(cost_label(CombatAction::HeavyStrike), "-15 stamina");
        assert_eq!(cost_label(CombatAction::Block), "-3 stamina");
        assert_eq!(cost_label(CombatAction::Rest), "+20 stamina");
        assert_eq!(cost_label(CombatAction::StepForward), "poziție");
        assert_eq!(cost_label(CombatAction::StepBack), "poziție");
        assert_eq!(cost_label(CombatAction::LeapForward), "poziție");
    }

    // --- headless screen behavior ---

    #[test]
    fn entering_fight_spawns_the_full_hud() {
        let mut app = test_app();
        let roots = app
            .world_mut()
            .query_filtered::<(), With<HudScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 1, "one HUD root");
        let buttons = app
            .world_mut()
            .query_filtered::<(), (With<ActionButton>, With<Button>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            buttons, 7,
            "four combat buttons plus three movement buttons"
        );
        let fills = app
            .world_mut()
            .query::<&BarFill>()
            .iter(app.world())
            .count();
        assert_eq!(fills, 4, "hp + stamina fill per fighter");
        let logs = app
            .world_mut()
            .query_filtered::<(), With<LogText>>()
            .iter(app.world())
            .count();
        assert_eq!(logs, 1, "one log text node");
        assert!(app.world().get_resource::<CombatLog>().is_some());
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
    fn narrowing_the_viewport_grows_the_action_buttons_and_wraps_the_grid() {
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

        let action_bar_wrap = app
            .world_mut()
            .query_filtered::<&Node, With<ActionBarRoot>>()
            .single(app.world())
            .expect("one action bar root")
            .flex_wrap;
        assert_eq!(action_bar_wrap, FlexWrap::Wrap, "2x2 grid under mobile");

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
    fn panels_show_names_and_current_over_max_pools() {
        let mut app = test_app();
        assert_eq!(
            label_text(&mut app, HudLabel::Name(CombatSide::Player)),
            "Făt-Frumos"
        );
        assert_eq!(
            label_text(&mut app, HudLabel::Name(CombatSide::Enemy)),
            "Hoț de codru"
        );
        assert_eq!(
            label_text(
                &mut app,
                HudLabel::Pool {
                    side: CombatSide::Player,
                    pool: Pool::Health,
                }
            ),
            "90/90"
        );
        assert_eq!(
            label_text(
                &mut app,
                HudLabel::Pool {
                    side: CombatSide::Enemy,
                    pool: Pool::Stamina,
                }
            ),
            "40/40"
        );
    }

    #[test]
    fn the_player_panel_shows_the_level_next_to_the_name() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(CombatRng(strikes_rng(4)));
        app.insert_resource(Level {
            level: 3,
            xp: 40,
            unspent_points: 0,
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        assert_eq!(
            label_text(&mut app, HudLabel::Name(CombatSide::Player)),
            "Făt-Frumos (Nv. 3)"
        );
        assert_eq!(
            label_text(&mut app, HudLabel::Name(CombatSide::Enemy)),
            "Hoț de codru",
            "enemy panels show the roster name without a level"
        );
    }

    #[test]
    fn bar_fills_track_the_pools_every_frame_and_clamp() {
        let mut app = test_app();
        assert_eq!(
            fill_width(&mut app, CombatSide::Enemy, Pool::Health),
            Val::Percent(100.0),
            "full at spawn"
        );
        set_enemy_health(&mut app, 35);
        app.update();
        assert_eq!(
            fill_width(&mut app, CombatSide::Enemy, Pool::Health),
            Val::Percent(50.0)
        );
        set_enemy_health(&mut app, 0);
        app.update();
        assert_eq!(
            fill_width(&mut app, CombatSide::Enemy, Pool::Health),
            Val::Percent(0.0)
        );
        set_enemy_health(&mut app, 140); // over the 70 max
        app.update();
        assert_eq!(
            fill_width(&mut app, CombatSide::Enemy, Pool::Health),
            Val::Percent(100.0),
            "overheal clamps to the track"
        );
        set_player_stamina(&mut app, 25); // max 50
        app.update();
        assert_eq!(
            fill_width(&mut app, CombatSide::Player, Pool::Stamina),
            Val::Percent(50.0)
        );
    }

    #[test]
    fn pressing_the_quick_strike_button_plays_the_action() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        press_button_and_wait(&mut app, CombatAction::QuickStrike);
        // Same expectations as the keyboard test in `systems`: a clean hit
        // for 6, the drained enemy rests, the turn returns to the player.
        assert_eq!(enemy_pools(&mut app), (64, 20));
        assert_eq!(player_pools(&mut app), (90, 45));
        assert_eq!(turn(&app).side, CombatSide::Player);
    }

    #[test]
    fn the_log_records_the_button_driven_exchange() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        press_button_and_wait(&mut app, CombatAction::QuickStrike);
        let lines: Vec<String> = app
            .world()
            .resource::<CombatLog>()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(
            lines,
            vec![
                "Făt-Frumos lovește pentru 6!",
                "Hoț de codru se odihnește și recuperează 20 stamina.",
            ]
        );
        assert_eq!(log_text(&mut app), lines.join("\n"), "text node in sync");
    }

    #[test]
    fn the_log_text_shows_the_last_eight_events_newest_at_the_bottom() {
        let mut app = test_app();
        for dmg in 1..=10 {
            app.world_mut().write_message(CombatLogEvent {
                actor: CombatSide::Player,
                action: CombatAction::QuickStrike,
                event: CombatEvent::Hit { dmg },
            });
        }
        app.update();
        let expected: Vec<String> = (3..=10)
            .map(|dmg| format!("Făt-Frumos lovește pentru {dmg}!"))
            .collect();
        assert_eq!(log_text(&mut app), expected.join("\n"));
    }

    #[test]
    fn buttons_disable_exactly_when_the_action_is_unavailable() {
        let mut app = test_app();
        set_player_stamina(&mut app, 10);
        app.update();

        let heavy = find_button(&mut app, CombatAction::HeavyStrike);
        assert!(
            app.world().entity(heavy).contains::<DisabledButton>(),
            "heavy strike greys out below its 15 cost"
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(heavy).map(|b| b.0),
            Some(BUTTON_DISABLED)
        );
        for affordable in [
            CombatAction::QuickStrike,
            CombatAction::Block,
            CombatAction::Rest,
            CombatAction::StepBack,
        ] {
            let button = find_button(&mut app, affordable);
            assert!(
                !app.world().entity(button).contains::<DisabledButton>(),
                "{affordable:?} stays enabled at 10 stamina"
            );
        }
        for unavailable in [CombatAction::StepForward, CombatAction::LeapForward] {
            let button = find_button(&mut app, unavailable);
            assert!(
                app.world().entity(button).contains::<DisabledButton>(),
                "{unavailable:?} greys out while already close"
            );
        }

        // A press on the disabled button is inert: no action resolves.
        let before = (player_pools(&mut app), enemy_pools(&mut app));
        press_button(&mut app, CombatAction::HeavyStrike);
        assert_eq!((player_pools(&mut app), enemy_pools(&mut app)), before);
        assert_eq!(turn(&app).side, CombatSide::Player, "turn did not pass");
    }

    #[test]
    fn a_fight_is_playable_start_to_finish_with_the_mouse_only() {
        let mut app = test_app();
        for _ in 0..200 {
            if turn(&app).over {
                break;
            }
            // Keep the enemy drained so it can only Rest; quick-strike when
            // affordable, otherwise rest — mouse clicks only.
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
        assert!(
            log_text(&mut app).contains("Hoț de codru este învins!"),
            "the defeat is logged"
        );
        for action in [
            CombatAction::QuickStrike,
            CombatAction::HeavyStrike,
            CombatAction::Block,
            CombatAction::Rest,
            CombatAction::StepForward,
            CombatAction::StepBack,
            CombatAction::LeapForward,
        ] {
            let button = find_button(&mut app, action);
            assert!(
                app.world().entity(button).contains::<DisabledButton>(),
                "{action:?} greys out once the duel is over"
            );
        }
    }

    #[test]
    fn leaving_the_fight_despawns_the_hud_and_drops_the_log() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        let hud = app
            .world_mut()
            .query_filtered::<(), With<HudScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(hud, 0, "HUD root and children despawned");
        let buttons = app
            .world_mut()
            .query_filtered::<(), With<ActionButton>>()
            .iter(app.world())
            .count();
        assert_eq!(buttons, 0);
        assert!(app.world().get_resource::<CombatLog>().is_none());
    }
}
