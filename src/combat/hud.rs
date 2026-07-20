//! Combat HUD for the fight screen: per-fighter health/stamina panels, the
//! scrolling combat log, and the HUD "shell" (root sizing, letterbox
//! tracking, the responsive mobile/desktop breakpoint). The action palette
//! itself — desktop's flat row and phone's category disclosure alike — is
//! owned by [`super::action_palette`] (#189/#199): [`spawn_hud`] delegates to
//! [`super::action_palette::spawn_action_bar`] instead of building buttons
//! here, and [`apply_responsive_hud_layout`] never touches [`ActionBarRoot`]
//! at all — its full responsive behavior (structure, container `Node`, and
//! per-button sizing) lives in
//! `action_palette::rebuild_action_bar_on_breakpoint_change`.
//!
//! The pure pieces (bar percentages and the event → log-line formatting) are
//! plain functions so they stay unit-testable and reusable — the announcer
//! issue builds its flavor text on top of the same [`CombatLogEvent`]
//! stream, so [`log_line`] keeps to plain factual wording.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::character::{EnemyFighter, FighterName, Health, PlayerFighter, Stamina};
use crate::core::{LetterboxRect, UiFont, ViewportInfo};
use crate::progression::Level;
use crate::roster::Boss;
use crate::theme::{BUTTON_NORMAL, CREAM, MOBILE_LOG_LINES, Palette, PanelTexture, panel_bundle};
use crate::ui_widgets::focus::{Focusable, TabGroup, TabIndex};
// Only used by the desktop-strip fit check and its test (#120): the runtime
// paths never need to reason about the border inset directly.
#[cfg(test)]
use crate::theme::PANEL_BORDER_INSET;

use super::action_palette;
use super::actions::ExtraDescriptors;
use super::engine::{CombatEvent, DuelDistance};
use super::systems::{CombatLogEvent, CombatSide};

/// How many log lines the combat log keeps and shows.
pub const LOG_CAPACITY: usize = 8;

const PANEL_WIDTH: f32 = 240.0;
const BAR_HEIGHT: f32 = 16.0;

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
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's
/// `fight-palette-phone` browser scenario (#199) reads this marker's real
/// geometry to prove the phone palette never covers the fighter status
/// panels — the same visibility bump `action_palette::ActionButton` already
/// documents for the desktop scenario.
#[derive(Component)]
pub(crate) struct FighterPanelRoot;

/// Marker for the action-bar container: desktop's flat button row or
/// phone's category-disclosure stack (#199).
#[derive(Component)]
pub(super) struct ActionBarRoot;

/// Marker for the combat-log panel, repositioned/resized under the mobile
/// breakpoint so it doesn't overlap the taller 2×2 action grid.
///
/// `pub(crate)`, not `pub(super)`: the `review` feature's `fight-palette-phone`
/// browser scenario (#276) reads this marker's real geometry to prove the
/// phone palette never covers the combat log — the same visibility bump
/// `FighterPanelRoot` already documents for the fighter status panels.
#[derive(Component)]
pub(crate) struct LogPanelRoot;

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

/// Romanian name of one [`DuelDistance`] band — the shared distance
/// vocabulary for every readout of the engine's spacing (the arena's ground
/// distance chip today; any future HUD text reuses the same words).
pub fn distance_label(distance: DuelDistance) -> &'static str {
    match distance.band() {
        band if band == DuelDistance::CLOSE.band() => "Aproape",
        band if band == DuelDistance::NEAR.band() => "Aproximativ",
        _ => "Departe",
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
///
/// The root node is sized and positioned from [`LetterboxRect`] (#125)
/// instead of a full-window `Val::Percent(100.0)`, so every corner-anchored
/// child (nameplates, the action bar) lands inside the same rect the arena
/// art occupies rather than bleeding onto the letterbox bars.
/// [`apply_letterbox_to_hud_root`] keeps it in sync across window resizes.
pub(super) fn spawn_hud(
    mut commands: Commands,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    viewport: Res<ViewportInfo>,
    letterbox: Res<LetterboxRect>,
    extra_descriptors: Res<ExtraDescriptors>,
    palette: Res<Palette>,
) {
    commands.insert_resource(CombatLog::default());
    let is_mobile = viewport.is_mobile;
    commands
        .spawn((
            HudScreen,
            hud_root_node(&letterbox),
            children![
                fighter_panel(
                    CombatSide::Player,
                    &ui_font,
                    &panel_texture,
                    is_mobile,
                    &palette
                ),
                fighter_panel(
                    CombatSide::Enemy,
                    &ui_font,
                    &panel_texture,
                    is_mobile,
                    &palette
                ),
                pause_button(&ui_font),
                log_panel(&ui_font, &panel_texture, is_mobile, &palette),
            ],
        ))
        .with_children(|parent| {
            // #189: the action bar iterates action descriptors instead of a
            // hard-coded seven-button list — see `action_palette`'s module
            // docs for the full contract.
            action_palette::spawn_action_bar(
                parent,
                &ui_font,
                &panel_texture,
                is_mobile,
                &extra_descriptors,
                &viewport,
                &letterbox,
            );
        });
}

/// The HUD root's `Node`: absolutely positioned and sized to the letterboxed
/// stage rect (#125) rather than the full window, so the whole HUD — and
/// every child anchored to one of its corners — stays inside the same
/// centered 4:3 rect the world camera draws the arena into.
fn hud_root_node(letterbox: &LetterboxRect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(letterbox.position.x),
        top: Val::Px(letterbox.position.y),
        width: Val::Px(letterbox.size.x),
        height: Val::Px(letterbox.size.y),
        ..default()
    }
}

/// Re-fits the HUD root to [`LetterboxRect`] whenever it changes (a window
/// resize) — the counterpart of [`apply_responsive_hud_layout`] for the
/// letterbox stage bounds instead of the mobile breakpoint (#125).
pub(super) fn apply_letterbox_to_hud_root(
    letterbox: Res<LetterboxRect>,
    mut roots: Query<&mut Node, With<HudScreen>>,
) {
    if !letterbox.is_changed() {
        return;
    }
    for mut node in &mut roots {
        node.left = Val::Px(letterbox.position.x);
        node.top = Val::Px(letterbox.position.y);
        node.width = Val::Px(letterbox.size.x);
        node.height = Val::Px(letterbox.size.y);
    }
}

/// The small, touch-friendly ⏸ button top-center of the HUD; clicking it
/// opens the pause overlay (see [`super::pause`]).
///
/// Wrapped in its own `TabGroup::new(-1)` (#216) rather than carrying
/// `Focusable`/`TabIndex` directly on the absolutely-positioned outer node:
/// [`super::action_palette`]'s action bar is a *sibling* under [`HudScreen`]
/// with its own `TabGroup::new(0)`, and `bevy_input_focus`'s tab-navigation
/// gathers focusable entities by walking into every `TabGroup` it finds
/// world-wide, not just root-level ones -- nesting this button's group
/// *inside* `HudScreen` (rather than as a same-level sibling of the palette's
/// group) would make the palette's own buttons gathered twice (once via the
/// outer group's traversal, once via its own top-level entry), duplicating
/// them in tab order. Keeping this a same-level sibling group, ordered `-1`
/// (before the palette's `0`, matching its top-of-screen visual position),
/// avoids that entirely: the wrapper carries the exact absolute-position
/// `Node` the button used to have, sized to fill it, so the inner button's
/// own rendering is unchanged.
fn pause_button(ui_font: &UiFont) -> impl Bundle {
    (
        // #216: one shared focus region for just this button — see
        // `crate::ui_widgets::focus`'s registration API and this function's
        // own doc comment for why it cannot simply join `HudScreen`'s tree
        // under a single group with the action palette.
        TabGroup::new(-1),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Percent(50.0),
            // Center the fixed-width button on the 50% anchor.
            margin: UiRect::left(Val::Px(-24.0)),
            width: Val::Px(48.0),
            height: Val::Px(48.0),
            ..default()
        },
        children![(
            Button,
            super::pause::PauseButton,
            Focusable,
            TabIndex(0),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
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
///
/// Text/fill colors are read from `palette` at spawn time (#214) rather than
/// hardcoded, so a fight entered while high contrast is already on renders
/// correctly from the first frame; [`sync_hud_palette`] keeps them in sync
/// if the preference flips while the panel is already alive.
fn fighter_panel(
    side: CombatSide,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
    palette: &Palette,
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
                TextColor(palette.text_primary),
                HudLabel::Name(side),
            ),
            bar(side, Pool::Health, ui_font, palette),
            bar(side, Pool::Stamina, ui_font, palette),
        ],
    )
}

/// A thin gold edge, per the palette, drawn on carved-wood bar tracks.
const BAR_EDGE: Color = crate::theme::GOLD;

/// Marker on a bar's carved-wood track node, so [`sync_hud_palette`] can
/// find it independently of the [`BarFill`] child it wraps.
#[derive(Component)]
pub(super) struct BarTrackNode;

/// One bar row: a carved-wood track with a thin gold edge and a colored
/// fill, plus a `current/max` label. Fill/track/label colors come from
/// `palette` (#214: [`Palette::hp_fill`]/[`Palette::stamina_fill`] for the
/// fill, [`Palette::bar_track`] for the track, [`Palette::text_primary`] for
/// the label).
fn bar(side: CombatSide, pool: Pool, ui_font: &UiFont, palette: &Palette) -> impl Bundle {
    let fill_color = match pool {
        Pool::Health => palette.hp_fill,
        Pool::Stamina => palette.stamina_fill,
    };
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
                BackgroundColor(palette.bar_track),
                BorderColor::all(BAR_EDGE),
                BarTrackNode,
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
                TextColor(palette.text_primary),
                HudLabel::Pool { side, pool },
            ),
        ],
    )
}

/// The right-side combat-log panel with a single multi-line text node. Under
/// the mobile breakpoint it moves up and narrows so it clears the taller 2×2
/// action grid at the bottom of the screen. Text color comes from
/// `palette.combat_log_text` (#214).
fn log_panel(
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    is_mobile: bool,
    palette: &Palette,
) -> impl Bundle {
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
            TextColor(palette.combat_log_text),
            LogText,
        )],
    )
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

/// Query for optional Boss component on the enemy fighter.
type EnemyWithMaybeBoss<'w, 's> = Query<'w, 's, Option<&'static Boss>, With<EnemyFighter>>;

/// Refreshes the name and `current/max` labels from the fighter components.
/// Skips frames where no fighter data changed *and* the palette didn't
/// switch (#214), so the string formatting only runs when a label could
/// actually differ. For boss opponents, tints the name label with
/// `palette.boss_label`; every other label uses `palette.text_primary`.
pub(super) fn update_labels(
    player: PlayerData,
    enemy: EnemyData,
    level: Option<Res<Level>>,
    enemy_boss: EnemyWithMaybeBoss,
    changed: Query<(), FighterDataChanged>,
    palette: Res<Palette>,
    mut labels: Query<(&HudLabel, &mut Text, &mut TextColor)>,
) {
    if changed.is_empty() && !palette.is_changed() {
        return;
    }
    let enemy_is_boss = enemy_boss.single().ok().flatten().is_some();
    for (label, mut text, mut color) in &mut labels {
        let side = match label {
            HudLabel::Name(side) | HudLabel::Pool { side, .. } => *side,
        };
        let Some((name, health, stamina)) = side_data(side, &player, &enemy) else {
            continue;
        };
        color.0 = match label {
            HudLabel::Name(CombatSide::Enemy) if enemy_is_boss => palette.boss_label,
            HudLabel::Name(_) | HudLabel::Pool { .. } => palette.text_primary,
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
/// to the last [`MOBILE_LOG_LINES`] under the mobile breakpoint (#31), and
/// keeps its color in sync with `palette.combat_log_text` (#214).
pub(super) fn update_log_text(
    log: Res<CombatLog>,
    viewport: Res<ViewportInfo>,
    palette: Res<Palette>,
    mut texts: Query<(&mut Text, &mut TextColor), With<LogText>>,
) {
    for (mut text, mut color) in &mut texts {
        let value = if viewport.is_mobile {
            log.to_text_capped(MOBILE_LOG_LINES)
        } else {
            log.to_text()
        };
        if text.0 != value {
            text.0 = value;
        }
        if color.0 != palette.combat_log_text {
            color.0 = palette.combat_log_text;
        }
    }
}

/// Keeps every bar track's [`BackgroundColor`] and each [`BarFill`]'s
/// [`BackgroundColor`] in sync with the active [`Palette`] (#214), so a
/// high-contrast toggle flipped while the HUD is already alive (e.g. from
/// the pause overlay's **Setări**, which never leaves `GameState::Fight`)
/// switches the HP/stamina bars immediately. A no-op frame unless `palette`
/// actually changed this frame.
pub(super) fn sync_hud_palette(
    palette: Res<Palette>,
    mut tracks: Query<&mut BackgroundColor, (With<BarTrackNode>, Without<BarFill>)>,
    mut fills: Query<(&BarFill, &mut BackgroundColor), Without<BarTrackNode>>,
) {
    if !palette.is_changed() {
        return;
    }
    for mut track in &mut tracks {
        track.0 = palette.bar_track;
    }
    for (fill, mut background) in &mut fills {
        background.0 = match fill.pool {
            Pool::Health => palette.hp_fill,
            Pool::Stamina => palette.stamina_fill,
        };
    }
}

/// Query filter for one responsive-layout node kind: it carries `Root` but
/// neither of the other two root markers, so the three queries in
/// [`apply_responsive_hud_layout`] never alias the same entity.
type ResponsiveNodeFilter<Root, A, B> = (With<Root>, Without<A>, Without<B>);

/// Re-flows the HUD when [`ViewportInfo`] crosses the mobile breakpoint:
/// resizes the fighter panels and the log panel in place instead of
/// respawning the whole HUD. The action bar is *not* handled here (unlike
/// pre-#199): desktop's flat row and phone's category disclosure are
/// structurally different layouts, not just a resized container, so its full
/// responsive behavior — structure, container `Node`, and per-button sizing
/// alike — now lives in
/// [`action_palette::rebuild_action_bar_on_breakpoint_change`], which
/// rebuilds the subtree fresh on a crossing instead of patching it in place.
pub(super) fn apply_responsive_hud_layout(
    viewport: Res<ViewportInfo>,
    mut panels: Query<
        &mut Node,
        ResponsiveNodeFilter<FighterPanelRoot, ActionBarRoot, LogPanelRoot>,
    >,
    mut logs: Query<&mut Node, ResponsiveNodeFilter<LogPanelRoot, FighterPanelRoot, ActionBarRoot>>,
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
    use super::super::action_palette::{ActionButton, ActionCostOrReason};
    use super::super::engine::{
        CombatAction, DuelDistance, HEAVY_STRIKE_BASE_HIT, QUICK_STRIKE_BASE_HIT,
    };
    use super::super::systems::{CombatPlugin, CombatRng, PlayerActionEvent};
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::Attributes;
    use crate::character::stats;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MIN};
    use crate::core::{CorePlugin, GameState};
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use crate::settings::AccessibilityPreferences;
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
        atac: 1,
        aparare: 2,
        carisma: 1,
        magie: 0,
    };

    #[test]
    fn every_distance_band_has_its_romanian_label() {
        assert_eq!(distance_label(DuelDistance::CLOSE), "Aproape");
        assert_eq!(distance_label(DuelDistance::NEAR), "Aproximativ");
        assert_eq!(distance_label(DuelDistance::FAR), "Departe");
    }

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

    fn advance_presentation(app: &mut App) {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(super::super::systems::PRESENTATION_DELAY_SECONDS + 0.1),
        ));
        app.update();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
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

    /// #120: every panel_bundle-decorated HUD node (fighter panels, the log
    /// panel, the action bar) must keep at least `PANEL_BORDER_INSET` of
    /// padding on all four sides, so nameplates, HP/stamina readouts, and the
    /// combat log never render on top of the embroidered border.
    #[test]
    fn hud_panels_clear_the_border_inset_on_every_side() {
        let mut app = test_app();

        fn assert_padding_clears_inset(node: &Node, label: &str) {
            for (side, val) in [
                ("left", node.padding.left),
                ("right", node.padding.right),
                ("top", node.padding.top),
                ("bottom", node.padding.bottom),
            ] {
                match val {
                    Val::Px(px) => assert!(
                        px >= PANEL_BORDER_INSET,
                        "{label} {side} padding {px} below the {PANEL_BORDER_INSET}px border inset"
                    ),
                    other => panic!("{label} {side} padding expected Val::Px, got {other:?}"),
                }
            }
        }

        let mut fighter_panels = app
            .world_mut()
            .query_filtered::<&Node, With<FighterPanelRoot>>();
        let mut count = 0;
        for node in fighter_panels.iter(app.world()) {
            assert_padding_clears_inset(node, "fighter panel");
            count += 1;
        }
        assert_eq!(count, 2, "one fighter panel per side");

        let log_node = app
            .world_mut()
            .query_filtered::<&Node, With<LogPanelRoot>>()
            .single(app.world())
            .expect("one log panel root");
        assert_padding_clears_inset(log_node, "log panel");

        let action_bar_node = app
            .world_mut()
            .query_filtered::<&Node, With<ActionBarRoot>>()
            .single(app.world())
            .expect("one action bar root");
        assert_padding_clears_inset(action_bar_node, "action bar");
    }

    /// Action-tile icon/dimension coverage and the desktop-strip fit check
    /// moved to `action_palette`'s own test module (#189) alongside the
    /// buttons themselves; the mobile breakpoint's structural rebuild (a flat
    /// row on desktop, category disclosure on phone — #199, replacing the
    /// pre-#199 in-place 2x2 grid resize this test used to check) has its own
    /// coverage there too (`action_palette::tests::phone_palette`), since
    /// that module now owns the action bar's full responsive behavior.
    #[test]
    fn narrowing_the_viewport_still_leaves_exactly_one_action_bar_root() {
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

        let action_bars = app
            .world_mut()
            .query_filtered::<(), With<ActionBarRoot>>()
            .iter(app.world())
            .count();
        assert_eq!(
            action_bars, 1,
            "the action bar rebuild must leave exactly one ActionBarRoot, never zero or two"
        );
    }

    /// Finds the `ActionButton` entity whose `intent` is `action`. Real
    /// button entities live under `action_palette` (#189/#199), but #124's
    /// acceptance test lives here per the issue's test expectations, so this
    /// mirrors `action_palette::tests::find_button_by_action` rather than
    /// importing a `cfg(test)`-only helper across module boundaries.
    fn find_action_button(app: &mut App, action: CombatAction) -> Entity {
        app.world_mut()
            .query::<(Entity, &ActionButton)>()
            .iter(app.world())
            .find(|(_, button)| button.intent == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button for {action:?} exists"))
    }

    /// The rendered sub-label text (cost/hit-chance, or the disabled reason)
    /// under `action`'s button — the same `ActionCostOrReason` node
    /// `action_palette::tests::find_cost_or_reason_text` reads.
    fn action_sublabel_text(app: &mut App, action: CombatAction) -> String {
        let entity = find_action_button(app, action);
        let children = app
            .world()
            .get::<Children>(entity)
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
        panic!("no ActionCostOrReason child found for {action:?}");
    }

    /// #124's acceptance test: `Lovitură iute` and `Lovitură grea` each show a
    /// hit percentage matching `stats::hit_percent` for the current matchup.
    /// The fixture player ties the first ladder opponent's `agilitate`, so
    /// each strike's percentage equals its own base hit chance exactly (see
    /// `PLAYER_ATTRIBUTES`'s doc comment).
    #[test]
    fn strike_buttons_show_the_hit_chance_matching_stats_hit_percent() {
        let mut app = test_app();
        let enemy_attrs = crate::roster::LADDER[0].attrs;

        let expected_quick =
            stats::hit_percent(&PLAYER_ATTRIBUTES, &enemy_attrs, QUICK_STRIKE_BASE_HIT);
        let quick_text = action_sublabel_text(&mut app, CombatAction::QuickStrike);
        assert!(
            quick_text.contains(&format!("{expected_quick}%")),
            "quick strike sub-label {quick_text:?} must show {expected_quick}%"
        );

        let expected_heavy =
            stats::hit_percent(&PLAYER_ATTRIBUTES, &enemy_attrs, HEAVY_STRIKE_BASE_HIT);
        let heavy_text = action_sublabel_text(&mut app, CombatAction::HeavyStrike);
        assert!(
            heavy_text.contains(&format!("{expected_heavy}%")),
            "heavy strike sub-label {heavy_text:?} must show {expected_heavy}%"
        );
    }

    /// #124's second acceptance test: non-attack actions never show a `%`.
    #[test]
    fn non_attack_buttons_show_no_percent_sign() {
        let mut app = test_app();
        for action in [CombatAction::Block, CombatAction::Rest] {
            let text = action_sublabel_text(&mut app, action);
            assert!(
                !text.contains('%'),
                "{action:?} sub-label {text:?} must not show a percent"
            );
        }
    }

    /// #125: the HUD root must track [`crate::core::LetterboxRect`] instead
    /// of spanning the full window, so a pillarboxed (non-4:3) viewport's
    /// nameplates and action bar stay inside the same rect as the arena art.
    #[test]
    fn hud_root_matches_the_letterbox_rect_for_a_non_4_3_viewport() {
        let mut app = test_app();

        // A 1280x800 (16:10) window's pillarboxed 4:3 rect.
        let rect = LetterboxRect {
            position: Vec2::new(107.0, 0.0),
            size: Vec2::new(1066.0, 800.0),
        };
        app.world_mut()
            .resource_mut::<LetterboxRect>()
            .set_if_neq(rect);
        app.update();

        let node = app
            .world_mut()
            .query_filtered::<&Node, With<HudScreen>>()
            .single(app.world())
            .expect("one HUD root");
        assert_eq!(node.left, Val::Px(rect.position.x));
        assert_eq!(node.top, Val::Px(rect.position.y));
        assert_eq!(node.width, Val::Px(rect.size.x));
        assert_eq!(node.height, Val::Px(rect.size.y));
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
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
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

    /// Button-driven combat resolution moved to `action_palette`'s own test
    /// module (#189); this one drives the same real turn resolution through
    /// a directly-written [`PlayerActionEvent`] instead, since it is really
    /// testing [`CombatLog`] content, not button-click mechanics.
    #[test]
    fn the_log_records_a_real_turn_exchange() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        app.world_mut()
            .write_message(PlayerActionEvent(CombatAction::QuickStrike));
        app.update();
        advance_presentation(&mut app);
        advance_presentation(&mut app);
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

    // Button-disable/reason coverage and the full mouse-only playthrough
    // moved to `action_palette`'s own test module (#189), where the buttons
    // themselves now live.

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
        assert!(app.world().get_resource::<CombatLog>().is_none());
    }

    #[test]
    fn regular_opponent_nameplate_uses_cream_text_color() {
        let mut app = test_app();
        // Find the enemy nameplate text
        let enemy_name_color = app
            .world_mut()
            .query::<(&HudLabel, &TextColor)>()
            .iter(app.world())
            .find(|(label, _)| **label == HudLabel::Name(CombatSide::Enemy))
            .map(|(_, color)| color.0)
            .expect("enemy nameplate exists");
        assert_eq!(
            enemy_name_color, CREAM,
            "regular opponent nameplate uses CREAM"
        );
    }

    #[test]
    fn boss_opponent_nameplate_uses_boss_label_color() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
        });
        app.insert_resource(crate::roster::LadderProgress(4)); // Muma Pădurii, the first boss
        app.insert_resource(CombatRng(strikes_rng(4)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();

        let enemy_name_color = app
            .world_mut()
            .query::<(&HudLabel, &TextColor)>()
            .iter(app.world())
            .find(|(label, _)| **label == HudLabel::Name(CombatSide::Enemy))
            .map(|(_, color)| color.0)
            .expect("enemy nameplate exists");
        assert_eq!(
            enemy_name_color,
            crate::theme::BOSS_LABEL_COLOR,
            "boss opponent nameplate uses BOSS_LABEL_COLOR"
        );
    }

    // --- Runtime-switchable palette (#214) ---

    /// A fight entered while high contrast is *already* on (e.g. toggled in
    /// a prior menu visit and persisted) spawns its bars, labels, and log
    /// with the high-contrast palette from the very first frame — not the
    /// normal-palette hardcoded literals a naive `spawn_hud` would use, and
    /// not requiring a second frame for `sync_hud_palette` to correct it
    /// (that system only fires on a *change*, and nothing changes here).
    #[test]
    fn hud_spawns_with_the_already_active_high_contrast_palette() {
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
        app.insert_resource(AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: true,
        });
        // Palette syncs to high-contrast this frame (still menu/creation
        // state, before the fight exists).
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        // OnEnter(Fight) spawns the HUD, reading the already-switched Palette.
        app.update();

        let track_color = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<BarTrackNode>>()
            .iter(app.world())
            .next()
            .expect("a bar track exists")
            .0;
        assert_eq!(track_color, Palette::high_contrast().bar_track);

        let fills: Vec<(Pool, Color)> = app
            .world_mut()
            .query::<(&BarFill, &BackgroundColor)>()
            .iter(app.world())
            .map(|(fill, bg)| (fill.pool, bg.0))
            .collect();
        assert_eq!(fills.len(), 4, "two fighters x two pools");
        for (pool, color) in fills {
            let expected = match pool {
                Pool::Health => Palette::high_contrast().hp_fill,
                Pool::Stamina => Palette::high_contrast().stamina_fill,
            };
            assert_eq!(color, expected, "{pool:?}");
        }

        let log_color = app
            .world_mut()
            .query_filtered::<&TextColor, With<LogText>>()
            .iter(app.world())
            .next()
            .expect("log text exists")
            .0;
        assert_eq!(log_color, Palette::high_contrast().combat_log_text);
    }

    /// Flipping high contrast *while the HUD is already alive* (the pause
    /// overlay's Setări never leaves `GameState::Fight`, see
    /// `settings::mod`'s docs) recolors the bars, track, labels, and log in
    /// place — no HUD respawn needed.
    #[test]
    fn toggling_high_contrast_mid_fight_recolors_bars_track_and_log() {
        let mut app = test_app();

        let track_before = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<BarTrackNode>>()
            .iter(app.world())
            .next()
            .expect("a bar track exists")
            .0;
        assert_eq!(track_before, Palette::normal().bar_track);

        app.insert_resource(AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: true,
        });
        app.update();

        let track_after = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<BarTrackNode>>()
            .iter(app.world())
            .next()
            .expect("a bar track exists")
            .0;
        assert_eq!(track_after, Palette::high_contrast().bar_track);

        let fills: Vec<(Pool, Color)> = app
            .world_mut()
            .query::<(&BarFill, &BackgroundColor)>()
            .iter(app.world())
            .map(|(fill, bg)| (fill.pool, bg.0))
            .collect();
        for (pool, color) in fills {
            let expected = match pool {
                Pool::Health => Palette::high_contrast().hp_fill,
                Pool::Stamina => Palette::high_contrast().stamina_fill,
            };
            assert_eq!(color, expected, "{pool:?}");
        }

        let log_color = app
            .world_mut()
            .query_filtered::<&TextColor, With<LogText>>()
            .iter(app.world())
            .next()
            .expect("log text exists")
            .0;
        assert_eq!(log_color, Palette::high_contrast().combat_log_text);

        let name_colors: Vec<Color> = app
            .world_mut()
            .query::<(&HudLabel, &TextColor)>()
            .iter(app.world())
            .filter(|(label, _)| matches!(label, HudLabel::Name(CombatSide::Player)))
            .map(|(_, color)| color.0)
            .collect();
        assert_eq!(
            name_colors,
            vec![Palette::high_contrast().text_primary],
            "non-boss nameplate follows the switched palette too"
        );
    }
}
