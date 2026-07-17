//! The shop screen ("Prăvălia lui Moș Pintea") for `GameState::Shop`: the
//! player spends galbeni on catalog gear between fights.
//!
//! Purchases live in the run-scoped [`OwnedItems`] set and the equipped
//! loadout in [`PlayerEquipment`]; both reset with the run like `Wallet`.
//! The pure [`try_buy`] holds the purchase rules so the UI systems stay thin.

use std::collections::HashSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::UiSystems;

use crate::character::{Attributes, PlayerAppearance, PlayerFighter, stats};
#[cfg(test)]
use crate::core::screen_point_for_world_point;
use crate::core::{
    GameState, LOGICAL_HEIGHT, LOGICAL_WIDTH, LetterboxRect, UiFont, ViewportInfo, despawn_screen,
    letterbox_zoom, logical_node_rect, world_point_for_screen_point,
};
use crate::creation::PlayerCharacter;
use crate::cutout::{CutoutRig, human_template_for, spawn_cutout_rig_with_gear};
use crate::flow::FlowIntent;
use crate::items::{CATALOG, Equipment, Item, ItemId, Slot};
use crate::menu::DisabledButton;
use crate::progression::Wallet;
use crate::save::{ResumeDestination, SaveRequested};
use crate::theme::{
    ARENA_BROWN, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, GOLD,
    MIN_TOUCH_TARGET, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
use crate::ui_widgets::focus::{
    FocusNavigationPlugin, FocusNavigationSet, Focusable, TabGroup, TabIndex,
};
use crate::ui_widgets::wide_button;

const SHOP_ROOT_PADDING: f32 = 12.0;
const SHOP_BODY_WIDTH: f32 = 760.0;
const SHOP_BODY_GAP: f32 = 12.0;
const SHOP_CATALOG_WIDTH: f32 = 430.0;
const SHOP_PREVIEW_STAGE_WIDTH: f32 = 318.0;
const SHOP_PANEL_HEIGHT: f32 = 450.0;
const SHOP_PREVIEW_FRAME_HEIGHT: f32 = 216.0;
const SHOP_PREVIEW_SCALE: f32 = 0.68;
/// World-space upward offset from the `PreviewStage` rect's projected
/// center, nudging the rig toward the cutout frame at the panel's top
/// (the loadout rows and stat strip fill the panel's lower half).
const SHOP_PREVIEW_Y: f32 = 70.0;
const SHOP_PREVIEW_Z: f32 = 25.0;

/// The items bought this run: a set of catalog ids. Persists across fights
/// within a run and resets with the run (see `progression::reset_run`).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnedItems(pub HashSet<ItemId>);

/// The player's persistent loadout: what the shop equips and the next
/// fight's player fighter wears. Lives as a resource because fighter
/// entities despawn between fights; resets with the run.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerEquipment(pub Equipment);

/// Why a purchase was refused; [`try_buy`] guarantees the wallet is left
/// untouched in every error case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyError {
    /// The item is already in [`OwnedItems`]; items are bought once.
    AlreadyOwned,
    /// The wallet holds less than the item's price.
    InsufficientFunds,
}

/// The pure purchase rule: rejects double-buys and underfunded buys, and on
/// success debits exactly the catalog price and marks the item owned.
/// Equipping is the caller's decision (the shop auto-equips).
pub fn try_buy(wallet: &mut Wallet, owned: &mut OwnedItems, id: ItemId) -> Result<(), BuyError> {
    if owned.0.contains(&id) {
        return Err(BuyError::AlreadyOwned);
    }
    let price = id.item().price;
    if wallet.0 < price {
        return Err(BuyError::InsufficientFunds);
    }
    wallet.0 -= price;
    owned.0.insert(id);
    Ok(())
}

/// Marker for the shop screen root; despawned by [`despawn_screen`] on
/// `OnExit(GameState::Shop)`.
#[derive(Component)]
pub struct ShopScreen;

#[derive(Component)]
struct ShopPreview;

/// Stable anchors for the shop's catalog, preview, and body-slot map.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum ShopLayoutRole {
    CatalogColumn,
    PreviewStage,
    LoadoutBodyMap,
    StatStrip,
}

/// What a shop button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopAction {
    /// Buy or equip the catalog item, depending on its current state.
    Item(ItemId),
    /// **Înapoi în arenă** → [`GameState::Fight`].
    BackToArena,
}

/// Which live piece of the shop screen a text label displays.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopLabel {
    /// The wallet balance in the top-right corner.
    Wallet,
    /// The summary panel's total attack line.
    Attack,
    /// The summary panel's total armor line.
    Armor,
    /// The summary panel's max HP line.
    Health,
    /// One line of the equipped-loadout preview.
    Loadout(Slot),
    /// The label of one item's buy/equip button.
    ItemButton(ItemId),
}

/// Icon purpose in the shop UI.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopIcon {
    kind: ShopIconKind,
}

/// Stable icon keys for path mapping and UI tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopIconKind {
    Wallet,
    Slot(Slot),
}

/// Romanian section header of one equipment slot in the catalog listing.
fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::Weapon => "Armă",
        Slot::Shield => "Scut",
        Slot::Torso => "Trup",
        Slot::Head => "Cap",
        Slot::Feet => "Picioare",
    }
}

/// Body attachment language shown next to each equipment slot.
fn slot_region_label(slot: Slot) -> &'static str {
    match slot {
        Slot::Weapon => "mână",
        Slot::Shield => "antebraț",
        Slot::Torso => "trunchi",
        Slot::Head => "cap",
        Slot::Feet => "picioare",
    }
}

/// Every icon kind the shop loads.
#[cfg(test)]
fn shop_icon_kinds() -> [ShopIconKind; 6] {
    [
        ShopIconKind::Wallet,
        ShopIconKind::Slot(Slot::Weapon),
        ShopIconKind::Slot(Slot::Shield),
        ShopIconKind::Slot(Slot::Torso),
        ShopIconKind::Slot(Slot::Head),
        ShopIconKind::Slot(Slot::Feet),
    ]
}

/// Asset path under `assets/` for one shop icon.
fn shop_icon_path(kind: ShopIconKind) -> &'static str {
    match kind {
        ShopIconKind::Wallet => "ui/icon_coin.png",
        ShopIconKind::Slot(Slot::Weapon) => "ui/icon_weapon.png",
        ShopIconKind::Slot(Slot::Shield) => "ui/icon_shield.png",
        ShopIconKind::Slot(Slot::Torso) => "ui/icon_torso.png",
        ShopIconKind::Slot(Slot::Head) => "ui/icon_head.png",
        ShopIconKind::Slot(Slot::Feet) => "ui/icon_feet.png",
    }
}

/// Loaded shop icon handles. Defaults keep headless tests working without an
/// asset server.
#[derive(Resource, Debug, Clone, Default)]
struct ShopIcons {
    wallet: Handle<Image>,
    weapon: Handle<Image>,
    shield: Handle<Image>,
    torso: Handle<Image>,
    head: Handle<Image>,
    feet: Handle<Image>,
}

impl ShopIcons {
    fn get(&self, kind: ShopIconKind) -> Handle<Image> {
        match kind {
            ShopIconKind::Wallet => self.wallet.clone(),
            ShopIconKind::Slot(Slot::Weapon) => self.weapon.clone(),
            ShopIconKind::Slot(Slot::Shield) => self.shield.clone(),
            ShopIconKind::Slot(Slot::Torso) => self.torso.clone(),
            ShopIconKind::Slot(Slot::Head) => self.head.clone(),
            ShopIconKind::Slot(Slot::Feet) => self.feet.clone(),
        }
    }
}

fn load_shop_icons(mut icons: ResMut<ShopIcons>, asset_server: Option<Res<AssetServer>>) {
    let Some(asset_server) = asset_server else {
        return;
    };
    icons.wallet = asset_server.load(shop_icon_path(ShopIconKind::Wallet));
    icons.weapon = asset_server.load(shop_icon_path(ShopIconKind::Slot(Slot::Weapon)));
    icons.shield = asset_server.load(shop_icon_path(ShopIconKind::Slot(Slot::Shield)));
    icons.torso = asset_server.load(shop_icon_path(ShopIconKind::Slot(Slot::Torso)));
    icons.head = asset_server.load(shop_icon_path(ShopIconKind::Slot(Slot::Head)));
    icons.feet = asset_server.load(shop_icon_path(ShopIconKind::Slot(Slot::Feet)));
}

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        // `Wallet` normally comes from `ProgressionPlugin`; init here too so
        // the shop is self-contained (idempotent when both plugins run).
        app.init_resource::<Wallet>()
            .init_resource::<OwnedItems>()
            .init_resource::<PlayerEquipment>()
            .init_resource::<ShopIcons>()
            .add_plugins((crate::ui_widgets::ScrollInputPlugin, FocusNavigationPlugin))
            .add_message::<SaveRequested>()
            .add_systems(PreStartup, load_shop_icons)
            .add_systems(
                OnEnter(GameState::Shop),
                (spawn_shop_screen, autosave_on_shop_entry),
            )
            .add_systems(
                Update,
                (
                    handle_shop_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    update_button_backgrounds,
                    refresh_shop_ui.run_if(
                        resource_changed::<Wallet>
                            .or_else(resource_changed::<OwnedItems>)
                            .or_else(resource_changed::<PlayerEquipment>),
                    ),
                    refresh_shop_preview_rig.run_if(resource_changed::<PlayerEquipment>),
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .chain()
                    .run_if(in_state(GameState::Shop)),
            )
            .add_systems(
                PostUpdate,
                update_shop_preview_transform
                    .after(UiSystems::Layout)
                    // So this frame's placement is reflected in
                    // `GlobalTransform` (and thus rendered) this same frame,
                    // rather than merely being ordered after layout with no
                    // guarantee relative to transform propagation.
                    .before(bevy::transform::TransformSystems::Propagate)
                    .run_if(in_state(GameState::Shop)),
            )
            .add_systems(OnExit(GameState::Shop), despawn_screen::<ShopScreen>)
            .add_systems(
                Update,
                dress_player_fighter
                    .before(crate::arena::ArenaSet::GearRefresh)
                    .run_if(in_state(GameState::Fight)),
            );
    }
}

/// The display state of one item's buy/equip button, derived from the
/// wallet, the owned set, and the equipped loadout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemButtonState {
    /// The item sits in its slot right now: a disabled marker.
    Equipped,
    /// Owned but not equipped: **Echipează** swaps the slot.
    Equip,
    /// Not owned and affordable: **Cumpără** is enabled.
    Buy,
    /// Not owned and the wallet is short: **Cumpără** is greyed out.
    TooExpensive,
}

impl ItemButtonState {
    fn of(id: ItemId, wallet: &Wallet, owned: &OwnedItems, equipment: &PlayerEquipment) -> Self {
        if equipment.0.equipped(id.item().slot) == Some(id) {
            Self::Equipped
        } else if owned.0.contains(&id) {
            Self::Equip
        } else if wallet.0 >= id.item().price {
            Self::Buy
        } else {
            Self::TooExpensive
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Equipped => "Echipat",
            Self::Equip => "Echipează",
            Self::Buy | Self::TooExpensive => "Cumpără",
        }
    }

    /// Whether the button reacts to presses; [`Self::Equipped`] and
    /// [`Self::TooExpensive`] are inert markers.
    fn enabled(self) -> bool {
        matches!(self, Self::Equip | Self::Buy)
    }

    fn background(self) -> Color {
        if self.enabled() {
            BUTTON_NORMAL
        } else {
            BUTTON_DISABLED
        }
    }

    fn text_color(self) -> Color {
        if self.enabled() { CREAM } else { TEXT_DISABLED }
    }
}

/// The wallet line in the top-right corner.
fn wallet_text(wallet: &Wallet) -> String {
    format!("Pungă: {} galbeni", wallet.0)
}

/// One item's stat column: weapons show attack, armor pieces show armor.
fn stat_text(item: &Item) -> String {
    if item.slot == Slot::Weapon {
        format!("+{} atac", item.damage)
    } else {
        format!("+{} armură", item.armor)
    }
}

/// The summary panel's attack line: base damage plus equipped weapon bonus.
fn attack_text(attributes: &Attributes, equipment: &PlayerEquipment) -> String {
    format!(
        "Atac: {}",
        stats::base_damage(attributes) + equipment.0.total_damage_bonus()
    )
}

/// The summary panel's armor line: the equipped pieces' total.
fn armor_text(equipment: &PlayerEquipment) -> String {
    format!("Armură: {}", equipment.0.total_armor())
}

/// The summary panel's max HP line.
fn health_text(attributes: &Attributes) -> String {
    format!("Viață: {}", stats::max_hp(attributes))
}

/// The player's confirmed attributes, or the default build (with a warning)
/// if the flow was driven into the shop without a character.
fn player_attributes(player: Option<&PlayerCharacter>) -> Attributes {
    match player {
        Some(player) => player.attributes,
        None => {
            warn!("in GameState::Shop without a PlayerCharacter; showing the default build");
            Attributes::default()
        }
    }
}

fn player_appearance(player: Option<&PlayerCharacter>) -> PlayerAppearance {
    player.map(|player| player.appearance).unwrap_or_default()
}

#[derive(SystemParam)]
struct ShopScreenAssets<'w> {
    ui_font: Res<'w, UiFont>,
    panel_texture: Res<'w, PanelTexture>,
    icons: Res<'w, ShopIcons>,
    asset_server: Option<Res<'w, AssetServer>>,
}

/// Spawns the shop screen: header with the wallet top-right, the catalog
/// grouped by slot, the live stat summary, and the back-to-arena button.
fn spawn_shop_screen(
    mut commands: Commands,
    wallet: Res<Wallet>,
    owned: Res<OwnedItems>,
    equipment: Res<PlayerEquipment>,
    player: Option<Res<PlayerCharacter>>,
    viewport: Res<ViewportInfo>,
    assets: ShopScreenAssets,
) {
    let ui_font = &*assets.ui_font;
    let panel_texture = &*assets.panel_texture;
    let icons = &*assets.icons;
    let appearance = player_appearance(player.as_deref());
    let attributes = player_attributes(player.as_deref());
    // The brown backdrop is a world-space sprite behind the preview rig
    // (z -40 < SHOP_PREVIEW_Z), not a `BackgroundColor` on the UI root: the
    // UI pass composites over the world pass as one layer, so a full-screen
    // UI fill would hide the world-rendered rig no matter what the preview
    // stage itself does (#273) -- the same backdrop treatment the creation
    // screen uses.
    commands.spawn((
        ShopScreen,
        Sprite::from_color(ARENA_BROWN, Vec2::new(LOGICAL_WIDTH, LOGICAL_HEIGHT)),
        Transform::from_xyz(0.0, 0.0, -40.0),
    ));
    commands
        .spawn((
            ShopScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(SHOP_ROOT_PADDING)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
            // #216: one shared focus region for the whole shop screen
            // (catalog buy buttons and the back button alike) — see
            // `crate::ui_widgets::focus`'s registration API.
            TabGroup::new(0),
        ))
        .with_children(|parent| {
            // Header row: title on the left, wallet balance top-right.
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(8.0)),
                    // #287: without this, the root's default flex-shrink=1
                    // lets this row compress below its natural content
                    // height whenever the shop's total content (header +
                    // wrapped body + back button) overflows the window --
                    // silently disagreeing with its own rendered size the
                    // same way the wrapped body did (see the body's own
                    // `flex_shrink: 0.0` below for the full explanation).
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|header| {
                    header.spawn((
                        Text::new("Prăvălia lui Moș Pintea"),
                        ui_font.text_font_bold(32.0),
                        TextColor(CREAM),
                    ));
                    spawn_icon_text_row(
                        header,
                        ShopIconKind::Wallet,
                        wallet_text(&wallet),
                        20.0,
                        ui_font,
                        icons,
                        Some(ShopLabel::Wallet),
                    );
                });
            parent
                .spawn(Node {
                    width: Val::Px(SHOP_BODY_WIDTH),
                    max_width: Val::Percent(96.0),
                    // #297: at phone widths the two panels stack as a plain
                    // column. #296 stacked them by letting a `FlexWrap::Wrap`
                    // row wrap onto two lines, but the wrapped row's
                    // *resolved height* only covered its first line -- the
                    // back button (this body's next sibling) rendered at
                    // that height, on top of the catalog column's first
                    // header, at every phone DPR (see issue #297's recapture
                    // evidence). A column never wraps, so its content height
                    // is reliably the sum of its children and the button
                    // genuinely lands below both panels. Desktop keeps the
                    // original side-by-side wrapped row every accepted
                    // desktop baseline renders from.
                    flex_direction: if viewport.is_mobile {
                        FlexDirection::Column
                    } else {
                        FlexDirection::Row
                    },
                    flex_wrap: if viewport.is_mobile {
                        FlexWrap::NoWrap
                    } else {
                        FlexWrap::Wrap
                    },
                    justify_content: JustifyContent::Center,
                    // The column's cross axis is horizontal: `Center` keeps
                    // the fixed-width preview stage horizontally centered,
                    // exactly where the wrapped row's `justify_content:
                    // Center` used to place it on its own line.
                    align_items: if viewport.is_mobile {
                        AlignItems::Center
                    } else {
                        AlignItems::FlexStart
                    },
                    column_gap: Val::Px(SHOP_BODY_GAP),
                    row_gap: Val::Px(10.0),
                    // #287: the root column is a scroll container
                    // (`overflow: Overflow::scroll_y()`), which exists
                    // precisely so content taller than the window scrolls
                    // instead of compressing -- but a scrollable ancestor
                    // also resets a flex item's *automatic* minimum size to
                    // zero, so without an explicit `flex_shrink: 0.0` the
                    // engine's default flex-shrink=1 was free to squeeze
                    // this row's resolved height below the sum of its two
                    // wrapped lines (catalog + preview stage) whenever the
                    // stacked mobile layout overflowed the window. The back
                    // button, laid out as this row's next sibling, then
                    // landed at that compressed height instead of below the
                    // row's actual, taller rendered content -- overlapping
                    // the preview panel/frame. Pinning this to zero keeps
                    // the row at its true content height and lets the root's
                    // own scrolling handle any overflow instead.
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|body| {
                    // #287: at phone widths the body's two panels stack (a
                    // plain column since #297) -- so whichever of the two
                    // spawns first lands at the top. The letterboxed 4:3
                    // world strip the preview rig's world camera can draw
                    // into sits roughly in the screen's vertical middle,
                    // with the header above eating into the space before
                    // it; landing the preview stage *first* (right after
                    // the header) is what lands its resolved rect inside
                    // that strip, mirroring why the creation screen never
                    // had this problem: its preview stage is already the
                    // first body item there. Desktop viewports lay the two
                    // side by side, so the spawn order there only controls
                    // left/right placement -- kept as catalog-left/
                    // preview-right, matching every accepted desktop
                    // baseline.
                    if viewport.is_mobile {
                        spawn_shop_preview_stage(body, &equipment, &attributes, ui_font, icons);
                        spawn_shop_catalog_column(
                            body,
                            &wallet,
                            &owned,
                            &equipment,
                            ui_font,
                            panel_texture,
                            icons,
                        );
                    } else {
                        spawn_shop_catalog_column(
                            body,
                            &wallet,
                            &owned,
                            &equipment,
                            ui_font,
                            panel_texture,
                            icons,
                        );
                        spawn_shop_preview_stage(body, &equipment, &attributes, ui_font, icons);
                    }
                });
            parent.spawn((
                wide_button("Înapoi în arenă", ui_font),
                ShopAction::BackToArena,
            ));
        });
    spawn_shop_preview(
        &mut commands,
        &equipment.0,
        appearance,
        assets.asset_server.as_deref(),
    );
}

/// The catalog column: every equipment slot's header plus its buyable items,
/// grouped by slot. Extracted from [`spawn_shop_screen`] so the body can
/// spawn it before or after [`spawn_shop_preview_stage`] depending on
/// [`ViewportInfo::is_mobile`] (#287).
#[allow(clippy::too_many_arguments)]
fn spawn_shop_catalog_column(
    body: &mut ChildSpawnerCommands,
    wallet: &Wallet,
    owned: &OwnedItems,
    equipment: &PlayerEquipment,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    icons: &ShopIcons,
) {
    body.spawn((
        Node {
            width: Val::Px(SHOP_CATALOG_WIDTH),
            max_width: Val::Percent(100.0),
            height: Val::Px(SHOP_PANEL_HEIGHT),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ShopLayoutRole::CatalogColumn,
        ScrollPosition::default(),
        crate::ui_widgets::Scrollable,
    ))
    .with_children(|catalog| {
        for slot in Slot::ALL {
            spawn_icon_text_row(
                catalog,
                ShopIconKind::Slot(slot),
                slot_label(slot).to_string(),
                18.0,
                ui_font,
                icons,
                None,
            );
            for item in CATALOG.iter().filter(|item| item.slot == slot) {
                let state = ItemButtonState::of(item.id, wallet, owned, equipment);
                spawn_item_row(catalog, item, state, ui_font, panel_texture);
            }
        }
    });
}

/// The preview stage: the cutout window frame (the world-rendered rig lands
/// here, see [`update_shop_preview_transform`]) plus the loadout body map and
/// stat strip beneath it. Extracted from [`spawn_shop_screen`] so the body
/// can spawn it before or after [`spawn_shop_catalog_column`] depending on
/// [`ViewportInfo::is_mobile`] (#287).
///
/// Unlike every other panel on this screen, this one deliberately does
/// **not** use `panel_bundle` or a `BackgroundColor` fill: the world-space
/// cutout rig it frames is rendered by the *world* camera, composited
/// underneath the *UI* camera's output -- so any opaque UI fill over the
/// frame's rect would hide the rig completely, no matter how the rig itself
/// is positioned or scaled (#273, the same root cause #123 fixed for the
/// creation screen). The loadout rows and stat chips below the frame keep
/// their own WALNUT backing for legibility; they never overlap the frame.
fn spawn_shop_preview_stage(
    body: &mut ChildSpawnerCommands,
    equipment: &PlayerEquipment,
    attributes: &Attributes,
    ui_font: &UiFont,
    icons: &ShopIcons,
) {
    body.spawn((
        Node {
            width: Val::Px(SHOP_PREVIEW_STAGE_WIDTH),
            max_width: Val::Percent(100.0),
            min_height: Val::Px(SHOP_PANEL_HEIGHT),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(14.0)),
            ..default()
        },
        ShopLayoutRole::PreviewStage,
    ))
    .with_children(|preview_panel| {
        // The cutout "window": no fill of its own, so the world-space rig
        // underneath (positioned to land here, see
        // `update_shop_preview_transform`) renders unobscured. Only the gold
        // outline remains (#273).
        preview_panel.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(SHOP_PREVIEW_FRAME_HEIGHT),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor::all(GOLD),
        ));
        spawn_loadout_preview(preview_panel, equipment, ui_font, icons);
        spawn_stat_strip(preview_panel, attributes, equipment, ui_font);
    });
}

fn spawn_shop_preview(
    commands: &mut Commands,
    equipment: &Equipment,
    appearance: PlayerAppearance,
    asset_server: Option<&AssetServer>,
) {
    // Placeholder until `update_shop_preview_transform` places it for real:
    // `PreviewStage`'s `ComputedNode`/`UiGlobalTransform` don't exist until
    // Bevy's own UI layout pass runs, which only happens later this same
    // frame (`PostUpdate`, after `OnEnter` has been applied) -- see that
    // system's doc comment (#273).
    let preview = commands
        .spawn((
            ShopScreen,
            ShopPreview,
            Transform::from_xyz(0.0, SHOP_PREVIEW_Y, SHOP_PREVIEW_Z)
                .with_scale(Vec3::splat(SHOP_PREVIEW_SCALE)),
        ))
        .id();
    spawn_cutout_rig_with_gear(
        commands,
        preview,
        human_template_for(appearance),
        asset_server,
        false,
        equipment,
    );
}

#[cfg(test)]
fn shop_layout_fits_width(viewport_width: f32) -> bool {
    let usable_width = viewport_width - SHOP_ROOT_PADDING * 2.0;
    let desktop_width = SHOP_CATALOG_WIDTH + SHOP_BODY_GAP + SHOP_PREVIEW_STAGE_WIDTH;
    desktop_width <= SHOP_BODY_WIDTH
        && SHOP_CATALOG_WIDTH.min(usable_width) <= usable_width
        && SHOP_PREVIEW_STAGE_WIDTH.min(usable_width) <= usable_width
}

/// Reads the `PreviewStage` node's resolved screen rect and repositions
/// every [`ShopPreview`] root so its projected screen position lands at
/// that rect's center (offset up by [`SHOP_PREVIEW_Y`] world units, toward
/// the cutout frame at the panel's top), scaling it so its *apparent*
/// on-screen size stays constant regardless of the letterbox zoom (matching
/// the UI panel's own fixed `Val::Px` size, which does not itself
/// grow/shrink with the world camera's zoom) -- see
/// [`world_point_for_screen_point`]'s doc comment for why this replaces the
/// old `viewport.width`-only placement (#273).
///
/// Runs unconditionally (not gated on a resource-changed check): a
/// `ShopPreview` spawned this same frame (`OnEnter`) only gets a real
/// `PreviewStage` `ComputedNode`/`UiGlobalTransform` partway through this
/// very `PostUpdate`, once Bevy's own UI layout pass has run for it -- so
/// the first correct placement has to land on an ordinary frame, not a
/// change-detected one. Ordered `.after(UiSystems::Layout)` so it always
/// reads this frame's freshly resolved layout, never a stale one.
fn update_shop_preview_transform(
    letterbox: Res<LetterboxRect>,
    stage_nodes: Query<(&ComputedNode, &UiGlobalTransform, &ShopLayoutRole)>,
    mut previews: Query<&mut Transform, With<ShopPreview>>,
) {
    let Some((node, transform)) = stage_nodes.iter().find_map(|(node, transform, role)| {
        (*role == ShopLayoutRole::PreviewStage).then_some((node, transform))
    }) else {
        return;
    };
    let stage_rect = logical_node_rect(transform, node);
    let target = world_point_for_screen_point(stage_rect.center(), *letterbox);
    let zoom = letterbox_zoom(*letterbox);
    for mut preview_transform in &mut previews {
        preview_transform.translation.x = target.x;
        preview_transform.translation.y = target.y + SHOP_PREVIEW_Y;
        preview_transform.translation.z = SHOP_PREVIEW_Z;
        preview_transform.scale = Vec3::splat(SHOP_PREVIEW_SCALE / zoom);
    }
}

/// A cream text line of the given font size.
fn line_text(label: String, font_size: f32, ui_font: &UiFont) -> impl Bundle {
    (
        Text::new(label),
        ui_font.text_font(font_size),
        TextColor(CREAM),
    )
}

/// A small icon image with a stable marker for tests. The neighboring text
/// is always present, so the UI stays readable while an image is loading.
fn icon_node(kind: ShopIconKind, icons: &ShopIcons) -> impl Bundle {
    (
        ShopIcon { kind },
        ImageNode::new(icons.get(kind)),
        Node {
            width: Val::Px(24.0),
            height: Val::Px(24.0),
            flex_shrink: 0.0,
            ..default()
        },
    )
}

fn spawn_icon_text_row(
    parent: &mut ChildSpawnerCommands,
    kind: ShopIconKind,
    label: String,
    font_size: f32,
    ui_font: &UiFont,
    icons: &ShopIcons,
    shop_label: Option<ShopLabel>,
) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            margin: UiRect::top(Val::Px(4.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(icon_node(kind, icons));
            let mut text = row.spawn(line_text(label, font_size, ui_font));
            if let Some(shop_label) = shop_label {
                text.insert(shop_label);
            }
        });
}

fn loadout_text(slot: Slot, equipment: &PlayerEquipment) -> String {
    let item = equipment
        .0
        .equipped(slot)
        .map(|id| id.item().name)
        .unwrap_or("liber");
    format!("{} / {}: {item}", slot_label(slot), slot_region_label(slot))
}

fn spawn_loadout_preview(
    parent: &mut ChildSpawnerCommands,
    equipment: &PlayerEquipment,
    ui_font: &UiFont,
    icons: &ShopIcons,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                width: Val::Percent(100.0),
                ..default()
            },
            ShopLayoutRole::LoadoutBodyMap,
        ))
        .with_children(|panel| {
            for slot in Slot::ALL {
                panel
                    .spawn((
                        Node {
                            height: Val::Px(34.0),
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            padding: UiRect::horizontal(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(WALNUT),
                    ))
                    .with_children(|row| {
                        row.spawn(icon_node(ShopIconKind::Slot(slot), icons));
                        row.spawn((
                            line_text(loadout_text(slot, equipment), 14.0, ui_font),
                            ShopLabel::Loadout(slot),
                        ));
                    });
            }
        });
}

fn spawn_stat_strip(
    parent: &mut ChildSpawnerCommands,
    attributes: &Attributes,
    equipment: &PlayerEquipment,
    ui_font: &UiFont,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(6.0),
                ..default()
            },
            ShopLayoutRole::StatStrip,
        ))
        .with_children(|strip| {
            strip.spawn(stat_chip(
                attack_text(attributes, equipment),
                ShopLabel::Attack,
                ui_font,
            ));
            strip.spawn(stat_chip(armor_text(equipment), ShopLabel::Armor, ui_font));
            strip.spawn(stat_chip(
                health_text(attributes),
                ShopLabel::Health,
                ui_font,
            ));
        });
}

fn stat_chip(label: String, shop_label: ShopLabel, ui_font: &UiFont) -> impl Bundle {
    (
        Node {
            width: Val::Px(90.0),
            height: Val::Px(36.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(WALNUT),
        children![(
            Text::new(label),
            ui_font.text_font(14.0),
            TextColor(CREAM),
            shop_label,
        )],
    )
}

/// One catalog row: name, stat, price, and the buy/equip/equipped button.
fn spawn_item_row(
    parent: &mut ChildSpawnerCommands,
    item: &Item,
    state: ItemButtonState,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
) {
    parent
        .spawn(panel_bundle(
            panel_texture,
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                margin: UiRect::vertical(Val::Px(2.0)),
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((
                column(144.0),
                line_text(item.name.to_string(), 14.0, ui_font),
            ));
            row.spawn((column(62.0), line_text(stat_text(item), 14.0, ui_font)));
            row.spawn((
                column(70.0),
                line_text(format!("{} g", item.price), 14.0, ui_font),
            ));
            let mut button = row.spawn((
                Button,
                Focusable,
                TabIndex(0),
                Node {
                    width: Val::Px(92.0),
                    // ≥44px touch target (#31), up from the original 26px
                    // mouse-only height.
                    height: Val::Px(MIN_TOUCH_TARGET),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(state.background()),
                ShopAction::Item(item.id),
            ));
            if !state.enabled() {
                button.insert(DisabledButton);
            }
            button.with_children(|button| {
                button.spawn((
                    Text::new(state.label()),
                    ui_font.text_font(14.0),
                    TextColor(state.text_color()),
                    ShopLabel::ItemButton(item.id),
                ));
            });
        });
}

/// A fixed-width row cell so the catalog columns line up.
fn column(width: f32) -> Node {
    Node {
        width: Val::Px(width),
        ..default()
    }
}

/// Query filter: buttons whose interaction changed this frame.
type ChangedButton = (Changed<Interaction>, With<Button>);

/// Query filter: like [`ChangedButton`], but skipping disabled buttons.
type ChangedEnabledButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

/// Autosaves immediately on arriving in the shop -- both routes that reach
/// [`GameState::Shop`] (`FightResult`'s **La prăvălie** and `Victory`'s
/// **Turul 2**, see `crate::flow`'s transition table) are safe checkpoints in
/// their own right (#217): the shop is the resume destination even before
/// any purchase or equip happens here, so a reload right after arriving still
/// resumes at the shop with every current run value intact, not stranded at
/// whatever destination the *previous* checkpoint stored.
fn autosave_on_shop_entry(mut save_requests: MessageWriter<SaveRequested>) {
    save_requests.write(SaveRequested(ResumeDestination::Shop));
}

/// Runs the [`ShopAction`] of whichever shop button was pressed: buys (via
/// [`try_buy`]) and auto-equips, equips owned items, or emits
/// [`FlowIntent::BackToArena`] to leave for the fight. State is re-derived
/// from the resources on every press, so a stale-looking button can never
/// overdraw the wallet. Every successful purchase or equip swap autosaves
/// the run (see [`crate::save`]), tagged [`ResumeDestination::Shop`], before
/// any intent is written.
fn handle_shop_actions(
    interactions: Query<(&Interaction, &ShopAction), ChangedButton>,
    mut wallet: ResMut<Wallet>,
    mut owned: ResMut<OwnedItems>,
    mut equipment: ResMut<PlayerEquipment>,
    mut intents: MessageWriter<FlowIntent>,
    mut save_requests: MessageWriter<SaveRequested>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            ShopAction::BackToArena => {
                intents.write(FlowIntent::BackToArena);
            }
            ShopAction::Item(id) => {
                match ItemButtonState::of(id, &wallet, &owned, &equipment) {
                    ItemButtonState::Equip => {
                        equipment.0.equip(id);
                        save_requests.write(SaveRequested(ResumeDestination::Shop));
                    }
                    ItemButtonState::Buy => {
                        if try_buy(&mut wallet, &mut owned, id).is_ok() {
                            equipment.0.equip(id);
                            save_requests.write(SaveRequested(ResumeDestination::Shop));
                        }
                    }
                    // Inert markers; presses on them change nothing.
                    ItemButtonState::Equipped | ItemButtonState::TooExpensive => {}
                }
            }
        }
    }
}

/// Refreshes every live label and button state after the wallet, the owned
/// set, or the loadout changed. Runs after [`handle_shop_actions`] so the
/// screen reacts on the same frame as the click.
fn refresh_shop_ui(
    mut commands: Commands,
    wallet: Res<Wallet>,
    owned: Res<OwnedItems>,
    equipment: Res<PlayerEquipment>,
    player: Option<Res<PlayerCharacter>>,
    mut buttons: Query<(Entity, &ShopAction, &mut BackgroundColor), With<Button>>,
    mut labels: Query<(&mut Text, &mut TextColor, &ShopLabel)>,
) {
    for (entity, action, mut background) in &mut buttons {
        let &ShopAction::Item(id) = action else {
            continue;
        };
        let state = ItemButtonState::of(id, &wallet, &owned, &equipment);
        background.0 = state.background();
        if state.enabled() {
            commands.entity(entity).remove::<DisabledButton>();
        } else {
            commands.entity(entity).insert(DisabledButton);
        }
    }
    let attributes = player_attributes(player.as_deref());
    for (mut text, mut color, label) in &mut labels {
        let new = match *label {
            ShopLabel::Wallet => wallet_text(&wallet),
            ShopLabel::Attack => attack_text(&attributes, &equipment),
            ShopLabel::Armor => armor_text(&equipment),
            ShopLabel::Health => health_text(&attributes),
            ShopLabel::Loadout(slot) => loadout_text(slot, &equipment),
            ShopLabel::ItemButton(id) => {
                let state = ItemButtonState::of(id, &wallet, &owned, &equipment);
                color.0 = state.text_color();
                state.label().to_string()
            }
        };
        if text.0 != new {
            text.0 = new;
        }
    }
}

fn refresh_shop_preview_rig(
    equipment: Res<PlayerEquipment>,
    player: Option<Res<PlayerCharacter>>,
    mut commands: Commands,
    previews: Query<(Entity, Option<&Children>), With<ShopPreview>>,
    asset_server: Option<Res<AssetServer>>,
) {
    let appearance = player_appearance(player.as_deref());
    for (preview, children) in &previews {
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        commands.entity(preview).remove::<CutoutRig>();
        spawn_cutout_rig_with_gear(
            &mut commands,
            preview,
            human_template_for(appearance),
            asset_server.as_deref(),
            false,
            &equipment.0,
        );
    }
}

/// Hover/pressed background feedback for enabled shop buttons; disabled
/// buttons keep the greyed-out background [`refresh_shop_ui`] gave them.
fn update_button_backgrounds(
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

/// Copies the persistent [`PlayerEquipment`] loadout onto the player fighter
/// whenever the arena has one, so purchases show up in the next fight's
/// damage/armor numbers and test-driven loadout changes can refresh a live
/// fighter without arena-specific shop code.
fn dress_player_fighter(
    loadout: Res<PlayerEquipment>,
    mut fighters: Query<&mut Equipment, With<PlayerFighter>>,
) {
    for mut equipment in &mut fighters {
        if *equipment != loadout.0 {
            *equipment = loadout.0.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::combat::CombatLogEvent;
    use crate::core::CorePlugin;
    use crate::cutout::{CutoutPartKind, CutoutPartMarker, GearVisualLayer, cutout_rig_owner};
    use crate::flow::FlowPlugin;
    use crate::progression::{ProgressionPlugin, STARTING_GALBENI, result_ui::GameOverAction};
    use bevy::math::Affine2;
    use bevy::state::app::StatesPlugin;
    use bevy::window::PrimaryWindow;

    #[test]
    fn buying_without_funds_is_rejected_and_debits_nothing() {
        let mut wallet = Wallet(STARTING_GALBENI); // 50 < Paloș (150)
        let mut owned = OwnedItems::default();
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::Palos),
            Err(BuyError::InsufficientFunds)
        );
        assert_eq!(wallet, Wallet(STARTING_GALBENI), "no debit on rejection");
        assert!(owned.0.is_empty(), "nothing owned on rejection");
    }

    #[test]
    fn buying_debits_exactly_the_price_and_marks_the_item_owned() {
        let mut wallet = Wallet(STARTING_GALBENI);
        let mut owned = OwnedItems::default();
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::CaciulaDeOaie),
            Ok(())
        );
        assert_eq!(wallet, Wallet(STARTING_GALBENI - 10), "Căciula costs 10");
        assert!(owned.0.contains(&ItemId::CaciulaDeOaie));
    }

    #[test]
    fn buying_at_the_exact_price_empties_the_wallet() {
        let mut wallet = Wallet(10);
        let mut owned = OwnedItems::default();
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::CaciulaDeOaie),
            Ok(())
        );
        assert_eq!(wallet, Wallet(0), "exact price is affordable");
    }

    #[test]
    fn buying_one_galben_short_is_rejected() {
        let mut wallet = Wallet(9);
        let mut owned = OwnedItems::default();
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::CaciulaDeOaie),
            Err(BuyError::InsufficientFunds)
        );
        assert_eq!(wallet, Wallet(9));
    }

    #[test]
    fn double_buying_is_rejected_without_a_second_debit() {
        let mut wallet = Wallet(100);
        let mut owned = OwnedItems::default();
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::CaciulaDeOaie),
            Ok(())
        );
        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::CaciulaDeOaie),
            Err(BuyError::AlreadyOwned)
        );
        assert_eq!(wallet, Wallet(90), "only the first buy debits");
        assert_eq!(owned.0.len(), 1);
    }

    #[test]
    fn an_owned_item_stays_owned_when_another_takes_its_slot() {
        // Equip-swap behavior: buying a second weapon replaces the equipped
        // one but the first stays owned (no selling, no loss).
        let mut wallet = Wallet(200);
        let mut owned = OwnedItems::default();
        let mut equipment = Equipment::default();

        assert_eq!(
            try_buy(&mut wallet, &mut owned, ItemId::BataCiobaneasca),
            Ok(())
        );
        equipment.equip(ItemId::BataCiobaneasca);
        assert_eq!(try_buy(&mut wallet, &mut owned, ItemId::Palos), Ok(()));
        assert_eq!(
            equipment.equip(ItemId::Palos),
            Some(ItemId::BataCiobaneasca),
            "the swap returns the previously equipped weapon"
        );

        assert_eq!(wallet, Wallet(200 - 20 - 150));
        assert!(owned.0.contains(&ItemId::BataCiobaneasca), "still owned");
        assert!(owned.0.contains(&ItemId::Palos));
        assert_eq!(equipment.equipped(Slot::Weapon), Some(ItemId::Palos));
    }

    // --- headless UI tests ---

    /// Same player build as the arena/combat tests: base damage 6, HP 90.
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

    /// Headless app with only the shop flow.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            ShopPlugin,
        ));
        app.insert_resource(player_character());
        app.update();
        app
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn wallet(app: &App) -> u32 {
        app.world().resource::<Wallet>().0
    }

    fn owned(app: &App) -> &OwnedItems {
        app.world().resource::<OwnedItems>()
    }

    fn equipped(app: &App, slot: Slot) -> Option<ItemId> {
        app.world().resource::<PlayerEquipment>().0.equipped(slot)
    }

    /// Resolves which fighter/preview root owns each gear layer. Weapons,
    /// shields, and boots attach to hands, forearms, and feet, which are now
    /// nested several joints deep under their own parent part rather than
    /// being direct children of the rig root (#117), so ownership is
    /// resolved by climbing the chain via [`cutout_rig_owner`] instead of
    /// assuming a single `ChildOf` hop from the part to the root.
    fn gear_layers_for_owner(app: &mut App, owner: Entity) -> Vec<(ItemId, Slot, CutoutPartKind)> {
        let part_parents: std::collections::HashMap<Entity, Entity> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker, &ChildOf)>()
            .iter(app.world())
            .map(|(part, _, child_of)| (part, child_of.parent()))
            .collect();
        let part_kinds: std::collections::HashMap<Entity, CutoutPartKind> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker)>()
            .iter(app.world())
            .map(|(part, marker)| (part, marker.kind))
            .collect();
        let mut layers: Vec<(ItemId, Slot, CutoutPartKind)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let part = child_of.parent();
                let kind = *part_kinds.get(&part)?;
                let part_owner = cutout_rig_owner(part, |e| part_parents.get(&e).copied());
                (part_owner == owner).then_some((layer.item, layer.slot, kind))
            })
            .collect();
        layers.sort_by_key(|(item, _, _)| *item as usize);
        layers
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect()
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<C>>()
            .iter(app.world())
            .count()
    }

    /// The button entity carrying `action`.
    fn find_button(app: &mut App, action: ShopAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &ShopAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .expect("shop button exists")
    }

    /// Presses `button`: the handler runs on the first update, transitions
    /// and refreshes apply on the second.
    fn press(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    fn press_item_button(app: &mut App, id: ItemId) {
        let button = find_button(app, ShopAction::Item(id));
        press(app, button);
    }

    /// The current label of one item's buy/equip button.
    fn item_button_label(app: &mut App, id: ItemId) -> String {
        app.world_mut()
            .query::<(&Text, &ShopLabel)>()
            .iter(app.world())
            .find(|&(_, &label)| label == ShopLabel::ItemButton(id))
            .map(|(text, _)| text.0.clone())
            .expect("item button label exists")
    }

    fn is_disabled(app: &mut App, id: ItemId) -> bool {
        let button = find_button(app, ShopAction::Item(id));
        app.world().entity(button).contains::<DisabledButton>()
    }

    #[test]
    fn shop_icon_paths_cover_wallet_and_every_slot() {
        assert_eq!(shop_icon_path(ShopIconKind::Wallet), "ui/icon_coin.png");
        for slot in Slot::ALL {
            assert!(
                shop_icon_path(ShopIconKind::Slot(slot)).starts_with("ui/icon_"),
                "{slot:?} has an icon path"
            );
        }
    }

    #[test]
    fn every_shop_icon_asset_exists_on_disk() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for kind in shop_icon_kinds() {
            let path = manifest.join("assets").join(shop_icon_path(kind));
            assert!(
                path.is_file(),
                "{kind:?} icon missing at {}",
                path.display()
            );
        }
    }

    #[test]
    fn entering_the_shop_spawns_the_catalog_grouped_by_slot() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Prăvălia lui Moș Pintea".to_string()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"Pungă: 50 galbeni".to_string()),
            "{texts:?}"
        );
        for item in &CATALOG {
            assert!(texts.contains(&item.name.to_string()), "{texts:?}");
        }
        for slot in Slot::ALL {
            assert!(
                texts.contains(&slot_label(slot).to_string()),
                "missing {slot:?} header: {texts:?}"
            );
        }
        for slot in Slot::ALL {
            assert!(
                texts.contains(&loadout_text(slot, &PlayerEquipment::default())),
                "missing empty loadout slot {slot:?}: {texts:?}"
            );
        }
        assert!(texts.contains(&"Înapoi în arenă".to_string()), "{texts:?}");
        assert_eq!(
            count::<Button>(&mut app),
            CATALOG.len() + 1,
            "one button per item plus the back button"
        );
        assert_eq!(
            count::<ShopIcon>(&mut app),
            shop_icon_kinds().len() + Slot::ALL.len(),
            "wallet, slot headers, and loadout preview all show icons"
        );
    }

    #[test]
    fn shop_layout_maps_gear_slots_around_the_fighter_preview() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let roles: Vec<ShopLayoutRole> = app
            .world_mut()
            .query::<&ShopLayoutRole>()
            .iter(app.world())
            .copied()
            .collect();
        assert!(roles.contains(&ShopLayoutRole::CatalogColumn));
        assert!(roles.contains(&ShopLayoutRole::PreviewStage));
        assert!(roles.contains(&ShopLayoutRole::LoadoutBodyMap));
        assert!(roles.contains(&ShopLayoutRole::StatStrip));
        assert!(shop_layout_fits_width(375.0));

        let preview_stage = app
            .world_mut()
            .query::<(&Node, &ShopLayoutRole)>()
            .iter(app.world())
            .find(|(_, role)| **role == ShopLayoutRole::PreviewStage)
            .map(|(node, _)| node)
            .expect("shop preview stage exists");
        assert_eq!(preview_stage.width, Val::Px(SHOP_PREVIEW_STAGE_WIDTH));
        assert_eq!(preview_stage.min_height, Val::Px(SHOP_PANEL_HEIGHT));

        let screen_scroll_roots = app
            .world_mut()
            .query_filtered::<(), (With<ShopScreen>, With<crate::ui_widgets::Scrollable>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            screen_scroll_roots, 1,
            "stacked narrow shop layout must stay vertically reachable"
        );

        let texts = texts(&mut app);
        for slot in Slot::ALL {
            let expected = format!("{} / {}: liber", slot_label(slot), slot_region_label(slot));
            assert!(
                texts.contains(&expected),
                "missing body-region loadout row {expected}: {texts:?}"
            );
        }
    }

    /// Spawns a `Window`/[`PrimaryWindow`] of the given logical size (scale
    /// factor 1.0) so [`crate::core::letterbox_camera`] -- already wired by
    /// `CorePlugin` -- computes a real, non-default [`LetterboxRect`] for it,
    /// exactly like the running game. Headless test apps otherwise have no
    /// window at all, so `letterbox_camera` skips (see its `windows.single()`
    /// guard) and `LetterboxRect` stays at its unlettered default.
    fn spawn_primary_window(app: &mut App, width: f32, height: f32) {
        let mut window = Window::default();
        window.resolution = bevy::window::WindowResolution::new(width as u32, height as u32);
        app.world_mut().spawn((window, PrimaryWindow));
    }

    /// A full app on the shop screen with a real primary window of the given
    /// logical size, so [`LetterboxRect`] reflects genuine letterboxing
    /// (bars, zoom) instead of staying at its unlettered default -- the
    /// production code path #273 fixes only matters once there's an actual
    /// letterbox to project through.
    fn test_app_with_window(width: f32, height: f32) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            ShopPlugin,
        ));
        app.insert_resource(player_character());
        app.update();
        spawn_primary_window(&mut app, width, height);
        app.world_mut()
            .resource_mut::<ViewportInfo>()
            .set_if_neq(ViewportInfo {
                width,
                height,
                is_mobile: crate::theme::is_mobile_width(width),
            });
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Shop);
        app.update();
        app.update();
        app
    }

    /// The (sole) entity carrying `role` among the shop's layout markers.
    fn layout_role_entity(app: &mut App, role: ShopLayoutRole) -> Entity {
        app.world_mut()
            .query::<(Entity, &ShopLayoutRole)>()
            .iter(app.world())
            .find(|(_, r)| **r == role)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{role:?} exists"))
    }

    fn preview_stage_entity(app: &mut App) -> Entity {
        layout_role_entity(app, ShopLayoutRole::PreviewStage)
    }

    /// `entity`'s parent and its index among that parent's [`Children`] --
    /// used to prove sibling *order* (not just presence), e.g. for #287's
    /// mobile preview-stage-first reordering.
    fn sibling_position(app: &mut App, entity: Entity) -> (Entity, usize) {
        let parent = app
            .world()
            .get::<ChildOf>(entity)
            .expect("entity has a parent")
            .parent();
        let index = app
            .world()
            .get::<Children>(parent)
            .expect("parent has children")
            .iter()
            .position(|child| child == entity)
            .expect("entity is among its parent's children");
        (parent, index)
    }

    /// #287 defect 1, red-first: at phone widths the shop's wrapped body
    /// must place [`ShopLayoutRole::PreviewStage`] *before*
    /// [`ShopLayoutRole::CatalogColumn`]. The two never fit side by side at
    /// this width (the catalog alone already fills the wrapped row), so
    /// whichever spawns first lands on the body's first line, right after
    /// the header -- and that first line is what lands inside the
    /// letterboxed world strip the preview rig's world camera can actually
    /// draw into. Before this fix the catalog was always first, pushing the
    /// preview stage onto a second line below the strip, where the rig is
    /// structurally invisible no matter how it's positioned. This mirrors
    /// why the creation screen never had the problem: its preview stage is
    /// already the first (and only) body item there.
    #[test]
    fn phone_widths_place_the_preview_stage_before_the_catalog_column() {
        let mut app = test_app_with_window(375.0, 812.0);

        let catalog = layout_role_entity(&mut app, ShopLayoutRole::CatalogColumn);
        let preview = layout_role_entity(&mut app, ShopLayoutRole::PreviewStage);
        let (catalog_parent, catalog_index) = sibling_position(&mut app, catalog);
        let (preview_parent, preview_index) = sibling_position(&mut app, preview);

        assert_eq!(
            catalog_parent, preview_parent,
            "catalog and preview stage must share the wrapped body as their parent"
        );
        assert!(
            preview_index < catalog_index,
            "at phone widths the preview stage (index {preview_index}) must come before \
             the catalog column (index {catalog_index}) so it wraps onto the body's first \
             line, inside the letterboxed world strip, instead of being pushed below it (#287)"
        );
    }

    /// #287: desktop widths must keep the *original* catalog-first order --
    /// the two panels sit side by side there (never wrapping), so spawn
    /// order there only controls left/right placement, and every accepted
    /// desktop gold-journey baseline has the catalog on the left, the
    /// preview stage on the right. The phone-only reorder above must not
    /// leak into desktop layouts.
    #[test]
    fn desktop_widths_keep_the_catalog_column_before_the_preview_stage() {
        let mut app = test_app_with_window(1280.0, 800.0);

        let catalog = layout_role_entity(&mut app, ShopLayoutRole::CatalogColumn);
        let preview = layout_role_entity(&mut app, ShopLayoutRole::PreviewStage);
        let (_, catalog_index) = sibling_position(&mut app, catalog);
        let (_, preview_index) = sibling_position(&mut app, preview);

        assert!(
            catalog_index < preview_index,
            "desktop must keep the catalog (index {catalog_index}) before the preview stage \
             (index {preview_index}): unlike phone widths, they sit side by side and never \
             wrap, so reordering here would only swap their left/right placement, diffing \
             every accepted desktop-shop baseline for no reason (#287)"
        );
    }

    /// #287 defect 2, red-first: the header row and the wrapped body --
    /// [`ShopAction::BackToArena`]'s previous siblings under the scrollable
    /// root -- must never shrink below their natural content height. The
    /// root is a scroll container (`overflow: Overflow::scroll_y()`)
    /// specifically so content taller than the window scrolls instead of
    /// compressing, but a scrollable ancestor also resets a flex item's
    /// *automatic* minimum size to zero -- so without an explicit
    /// `flex_shrink: 0.0` on these two, the engine's default flex-shrink=1
    /// was free to squeeze the wrapped body's resolved height below the sum
    /// of its two stacked lines whenever the mobile layout's total content
    /// overflowed the window. The back button, laid out as the body's next
    /// sibling, then landed at that compressed height instead of below the
    /// body's actual, taller rendered content -- overlapping the preview
    /// panel/frame (visible in the accepted `phone-shop` baseline).
    #[test]
    fn header_and_body_never_shrink_below_their_content_height() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let catalog = layout_role_entity(&mut app, ShopLayoutRole::CatalogColumn);
        let (body, _) = sibling_position(&mut app, catalog);
        let (root, body_index) = sibling_position(&mut app, body);
        assert_eq!(
            body_index, 1,
            "the wrapped body must be the root's second child (header, body, back button)"
        );
        let root_children: Vec<Entity> = app
            .world()
            .get::<Children>(root)
            .expect("root has children")
            .iter()
            .collect();
        assert_eq!(
            root_children.len(),
            3,
            "the root must have exactly header, body, and the back button as children"
        );
        let header = root_children[0];

        let flex_shrink =
            |app: &App, entity: Entity| app.world().get::<Node>(entity).unwrap().flex_shrink;
        assert_eq!(
            flex_shrink(&app, body),
            0.0,
            "the wrapped body must not shrink below its content height (#287) -- otherwise \
             the back button, laid out right after it, is positioned using a compressed \
             height and overlaps the body's real, larger rendered content"
        );
        assert_eq!(
            flex_shrink(&app, header),
            0.0,
            "the header row must not shrink below its content height either, for the same \
             reason (#287)"
        );
    }

    /// #297 red-first: at phone widths the body must stack its two panels
    /// as a plain `FlexDirection::Column`, not a wrapped row. #296 already
    /// reordered the panels and pinned `flex_shrink: 0.0`, but the recapture
    /// run for its baselines (issue #297) showed the wrapped body's
    /// *resolved height* still only covered its first wrapped line: the
    /// back button -- the body's next sibling -- rendered at that height,
    /// on top of the catalog column's `Armă` header, at every phone DPR
    /// (see `gold-journey/phone{,-dpr2,-dpr3}-shop` actuals). A column
    /// never wraps, so its content height is reliably the sum of its
    /// children -- sidestepping the wrapped-row height computation
    /// entirely instead of fighting it.
    #[test]
    fn phone_widths_stack_the_body_as_a_column() {
        let mut app = test_app_with_window(375.0, 812.0);

        let catalog = layout_role_entity(&mut app, ShopLayoutRole::CatalogColumn);
        let (body, _) = sibling_position(&mut app, catalog);
        let node = app.world().get::<Node>(body).expect("body has a Node");

        assert_eq!(
            node.flex_direction,
            FlexDirection::Column,
            "at phone widths the body must be a plain column: a wrapped row's resolved \
             height only covers its first line, so the back button (the body's next \
             sibling) lands on top of the catalog column instead of below it (#297)"
        );
        assert_eq!(
            node.flex_wrap,
            FlexWrap::NoWrap,
            "the mobile column must not wrap -- wrapping is exactly the layout whose \
             resolved height excluded the second line (#297)"
        );
    }

    /// #297: desktop widths keep the original side-by-side wrapped row --
    /// the two panels fit on one line there, the resolved height is that
    /// line's height, and every accepted desktop gold-journey baseline
    /// renders from that layout. The phone-only column above must not leak
    /// into desktop.
    #[test]
    fn desktop_widths_keep_the_body_as_a_wrapped_row() {
        let mut app = test_app_with_window(1280.0, 800.0);

        let catalog = layout_role_entity(&mut app, ShopLayoutRole::CatalogColumn);
        let (body, _) = sibling_position(&mut app, catalog);
        let node = app.world().get::<Node>(body).expect("body has a Node");

        assert_eq!(
            node.flex_direction,
            FlexDirection::Row,
            "desktop must keep the side-by-side row layout every accepted desktop-shop \
             baseline renders from (#297)"
        );
        assert_eq!(
            node.flex_wrap,
            FlexWrap::Wrap,
            "desktop keeps the original wrap behavior (it never actually wraps at \
             1280x800; this preserves the accepted baselines' layout inputs) (#297)"
        );
    }

    /// Hand-supplies the `PreviewStage` node's resolved `ComputedNode`/
    /// `UiGlobalTransform` -- headless test apps never run Bevy's real
    /// `ui_layout_system` (no `RenderPlugin`/window-backed camera target), so
    /// this module's tests simulate "already laid out" the same way
    /// `creation`'s and `ui_widgets::focus`'s tests do, rather than relying
    /// on a live layout pass that doesn't happen here.
    fn set_preview_stage_rect(app: &mut App, rect: Rect) {
        let stage = preview_stage_entity(app);
        app.world_mut().entity_mut(stage).insert((
            ComputedNode {
                size: rect.size(),
                inverse_scale_factor: 1.0,
                ..Default::default()
            },
            UiGlobalTransform::from(Affine2::from_translation(rect.center())),
        ));
    }

    /// A plausible `PreviewStage` on-screen rect for a given viewport width:
    /// right of the catalog in the centered desktop body once both fit side
    /// by side, or centered on its own wrapped row once they no longer do --
    /// the same shape the real flexbox layout produces, but computed by hand
    /// for a test that (per [`set_preview_stage_rect`]'s doc comment) never
    /// runs the real layout system.
    fn sample_stage_rect(viewport_width: f32) -> Rect {
        let desktop_width = SHOP_CATALOG_WIDTH + SHOP_BODY_GAP + SHOP_PREVIEW_STAGE_WIDTH;
        let usable_width = viewport_width - SHOP_ROOT_PADDING * 2.0;
        let center_x = if usable_width >= desktop_width {
            viewport_width / 2.0 - SHOP_BODY_WIDTH / 2.0
                + SHOP_CATALOG_WIDTH
                + SHOP_BODY_GAP
                + SHOP_PREVIEW_STAGE_WIDTH / 2.0
        } else {
            viewport_width / 2.0
        };
        Rect::from_center_size(
            Vec2::new(center_x, 300.0),
            Vec2::new(SHOP_PREVIEW_STAGE_WIDTH, SHOP_PANEL_HEIGHT),
        )
    }

    fn shop_preview_transform(app: &mut App) -> Transform {
        *app.world_mut()
            .query_filtered::<&Transform, With<ShopPreview>>()
            .single(app.world())
            .expect("shop preview transform exists")
    }

    /// #273 red-first/green: the preview rig's `Transform`, once projected
    /// back to screen space through the same letterboxed camera math it was
    /// placed with, must land inside the `PreviewStage` node's *actual*
    /// resolved rect -- at desktop (1280x800), at the exact design
    /// resolution (800x600, no letterbox bars), and at a narrow mobile width
    /// (375x812) -- instead of the old `viewport.width`-only placement,
    /// which only ever happened to be correct at the exact design
    /// resolution.
    #[test]
    fn preview_rig_projects_inside_the_preview_stage_rect_at_several_widths() {
        for (width, height) in [
            (1280.0_f32, 800.0_f32),
            (LOGICAL_WIDTH, LOGICAL_HEIGHT),
            (375.0, 812.0),
        ] {
            let mut app = test_app_with_window(width, height);
            let stage_rect = sample_stage_rect(width);
            set_preview_stage_rect(&mut app, stage_rect);
            app.update();

            let letterbox = *app.world().resource::<LetterboxRect>();
            assert!(
                letterbox.size.x > 0.0,
                "at {width}x{height}: letterbox_camera must have computed a real rect"
            );
            let transform = shop_preview_transform(&mut app);
            let projected =
                screen_point_for_world_point(transform.translation.truncate(), letterbox);
            assert!(
                stage_rect.contains(projected),
                "at {width}x{height}: projected preview position {projected:?} must land \
                 inside the preview stage rect {stage_rect:?}"
            );
        }
    }

    /// The rig's apparent on-screen size must stay roughly constant
    /// regardless of the letterbox zoom, matching the UI panel's own fixed
    /// `Val::Px` size -- otherwise a wide desktop window (more zoom) would
    /// render the same loadout enormous next to an unchanged-size frame,
    /// and a narrow phone width would shrink it to a speck.
    #[test]
    fn preview_rig_scale_compensates_for_letterbox_zoom() {
        let mut wide = test_app_with_window(1280.0, 800.0);
        set_preview_stage_rect(&mut wide, sample_stage_rect(1280.0));
        wide.update();
        let mut narrow = test_app_with_window(375.0, 812.0);
        set_preview_stage_rect(&mut narrow, sample_stage_rect(375.0));
        narrow.update();

        let wide_zoom = letterbox_zoom(*wide.world().resource::<LetterboxRect>());
        let narrow_zoom = letterbox_zoom(*narrow.world().resource::<LetterboxRect>());
        assert!(wide_zoom > narrow_zoom, "sanity: wide window zooms in more");

        let wide_scale = shop_preview_transform(&mut wide).scale.x;
        let narrow_scale = shop_preview_transform(&mut narrow).scale.x;
        // Apparent size = world scale * zoom; must match within float noise.
        assert!(
            (wide_scale * wide_zoom - narrow_scale * narrow_zoom).abs() < 1e-4,
            "wide apparent size {} must match narrow apparent size {}",
            wide_scale * wide_zoom,
            narrow_scale * narrow_zoom
        );
    }

    /// #273's first root-cause half, proven directly: the rig must derive
    /// its position from the `PreviewStage` node's *real* resolved rect, not
    /// recompute an independent guess from `ViewportInfo::width` alone (the
    /// old `shop_preview_x_for_width`, which never looked at the node's
    /// `ComputedNode`/`UiGlobalTransform` at all). Two different, quite
    /// deliberately odd stage rects at the *same* viewport width must
    /// produce two different projected positions, each landing inside its
    /// own rect -- a width-keyed formula would produce the identical result
    /// both times and fail this.
    #[test]
    fn preview_rig_tracks_the_stage_rects_actual_position_not_a_width_keyed_guess() {
        let mut app = test_app_with_window(1280.0, 800.0);

        let odd_rect_one = Rect::from_center_size(Vec2::new(900.0, 120.0), Vec2::new(318.0, 450.0));
        set_preview_stage_rect(&mut app, odd_rect_one);
        app.update();
        let letterbox = *app.world().resource::<LetterboxRect>();
        let projected_one = screen_point_for_world_point(
            shop_preview_transform(&mut app).translation.truncate(),
            letterbox,
        );
        assert!(
            odd_rect_one.contains(projected_one),
            "must land inside the first rect {odd_rect_one:?}, got {projected_one:?}"
        );

        let odd_rect_two = Rect::from_center_size(Vec2::new(200.0, 600.0), Vec2::new(318.0, 450.0));
        set_preview_stage_rect(&mut app, odd_rect_two);
        app.update();
        let projected_two = screen_point_for_world_point(
            shop_preview_transform(&mut app).translation.truncate(),
            letterbox,
        );
        assert!(
            odd_rect_two.contains(projected_two),
            "must land inside the second rect {odd_rect_two:?}, got {projected_two:?}"
        );
        assert!(
            projected_one.distance(projected_two) > 100.0,
            "moving the stage rect must move the projected preview position, proving it's \
             derived from the node's actual resolved layout rather than a fixed/width-keyed guess"
        );
    }

    /// #273's second root-cause half: the world camera draws the rig *under*
    /// the UI camera's output, so any opaque UI fill spanning the preview
    /// frame's rect hides the rig no matter where it's positioned. The
    /// `PreviewStage` panel must not carry `panel_bundle`'s 9-slice
    /// `ImageNode` or a `BackgroundColor` fill, and the frame row (the node
    /// with the gold border) must keep only its border outline.
    #[test]
    fn preview_stage_keeps_no_opaque_fill_over_the_rig_frame() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        // `Node` requires a `BackgroundColor` (default fully transparent),
        // so "no fill" means alpha 0, not component absence.
        let background_alpha = |app: &App, entity: Entity| {
            app.world()
                .get::<BackgroundColor>(entity)
                .map_or(0.0, |background| background.0.alpha())
        };
        let stage = preview_stage_entity(&mut app);
        assert_eq!(
            background_alpha(&app, stage),
            0.0,
            "the preview stage must not paint over the world-rendered rig"
        );
        assert!(
            app.world().get::<ImageNode>(stage).is_none(),
            "the preview stage must not carry panel_bundle's opaque 9-slice image"
        );

        // The whole *ancestor chain* matters, not just the stage itself: the
        // UI pass composites over the world pass as one layer, so a fill on
        // the screen root (or any container above the stage) hides the rig
        // exactly like a fill on the frame would -- the shop's original
        // full-screen `BackgroundColor(ARENA_BROWN)` root did precisely
        // that. The brown backdrop must come from a world-space sprite
        // behind the rig instead (see the assertion below), the same way
        // the creation screen's does.
        let mut ancestor = stage;
        while let Some(child_of) = app.world().get::<ChildOf>(ancestor) {
            ancestor = child_of.parent();
            assert_eq!(
                background_alpha(&app, ancestor),
                0.0,
                "no ancestor of the preview stage may paint over the world-rendered rig"
            );
            assert!(
                app.world().get::<ImageNode>(ancestor).is_none(),
                "no ancestor of the preview stage may carry an opaque image fill"
            );
        }
        let world_backdrop_exists = app
            .world_mut()
            .query_filtered::<&Sprite, With<ShopScreen>>()
            .iter(app.world())
            .any(|sprite| sprite.color == ARENA_BROWN);
        assert!(
            world_backdrop_exists,
            "the shop's brown backdrop must be a world-space sprite behind the rig, \
             not a UI fill over it"
        );

        let children: Vec<Entity> = app
            .world()
            .get::<Children>(stage)
            .expect("stage has children")
            .iter()
            .collect();
        let frame = children
            .iter()
            .copied()
            .find(|&child| {
                app.world()
                    .get::<BorderColor>(child)
                    .is_some_and(|border| *border == BorderColor::all(GOLD))
            })
            .expect("the gold-bordered frame row exists");
        assert_eq!(
            background_alpha(&app, frame),
            0.0,
            "the cutout frame must keep only its border outline, not an opaque fill"
        );
    }

    #[test]
    fn equipment_changes_refresh_the_shop_cutout_preview() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let preview = app
            .world_mut()
            .query_filtered::<Entity, With<ShopPreview>>()
            .single(app.world())
            .expect("shop cutout preview exists");
        assert!(
            gear_layers_for_owner(&mut app, preview).is_empty(),
            "default shop loadout starts visually bare"
        );

        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::CoifDeOstean);
        app.insert_resource(PlayerEquipment(loadout));
        app.update();

        assert_eq!(
            gear_layers_for_owner(&mut app, preview),
            vec![
                (ItemId::Palos, Slot::Weapon, CutoutPartKind::HandFront),
                (ItemId::CoifDeOstean, Slot::Head, CutoutPartKind::Head),
            ]
        );
    }

    #[test]
    fn the_summary_panel_shows_the_derived_stats() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let texts = texts(&mut app);
        assert!(texts.contains(&"Atac: 6".to_string()), "2 + 4: {texts:?}");
        assert!(texts.contains(&"Armură: 0".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"Viață: 90".to_string()),
            "50 + 40: {texts:?}"
        );
    }

    #[test]
    fn buy_buttons_are_disabled_exactly_when_the_wallet_is_short() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        assert!(
            !is_disabled(&mut app, ItemId::CaciulaDeOaie),
            "10 galbeni is affordable with 50"
        );
        assert_eq!(
            item_button_label(&mut app, ItemId::CaciulaDeOaie),
            "Cumpără"
        );
        assert!(
            is_disabled(&mut app, ItemId::Palos),
            "150 galbeni is not affordable with 50"
        );
        assert_eq!(item_button_label(&mut app, ItemId::Palos), "Cumpără");
        let palos = find_button(&mut app, ShopAction::Item(ItemId::Palos));
        assert_eq!(
            app.world().get::<BackgroundColor>(palos).map(|b| b.0),
            Some(BUTTON_DISABLED),
            "unaffordable buys are greyed out"
        );
    }

    #[test]
    fn buying_debits_equips_and_updates_the_screen() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        press_item_button(&mut app, ItemId::CaciulaDeOaie); // 10 g, +1 armor

        assert_eq!(wallet(&app), 40, "debited exactly the price");
        assert!(owned(&app).0.contains(&ItemId::CaciulaDeOaie));
        assert_eq!(
            equipped(&app, Slot::Head),
            Some(ItemId::CaciulaDeOaie),
            "buying auto-equips"
        );
        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Pungă: 40 galbeni".to_string()),
            "the wallet label is live: {texts:?}"
        );
        assert!(
            texts.contains(&"Armură: 1".to_string()),
            "the summary is live: {texts:?}"
        );
        assert!(
            texts.contains(&"Cap / cap: Căciulă de oaie".to_string()),
            "the loadout preview is live: {texts:?}"
        );
        assert_eq!(
            item_button_label(&mut app, ItemId::CaciulaDeOaie),
            "Echipat"
        );
        assert!(
            is_disabled(&mut app, ItemId::CaciulaDeOaie),
            "the equipped item's button is a disabled marker"
        );
    }

    /// #216: Enter on the focused buy button must debit and equip exactly
    /// like a click -- see `buying_debits_equips_and_updates_the_screen` for
    /// the click version.
    #[test]
    fn enter_on_a_focused_buy_button_debits_and_equips_like_a_click() {
        let mut app = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        set_state(&mut app, GameState::Shop);

        let button = find_button(&mut app, ShopAction::Item(ItemId::CaciulaDeOaie));
        app.world_mut()
            .insert_resource(crate::ui_widgets::focus::InputFocus::from_entity(button));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(wallet(&app), 40, "debited exactly the price");
        assert!(owned(&app).0.contains(&ItemId::CaciulaDeOaie));
    }

    /// #216: every focusable catalog buy button and the **Înapoi în arenă**
    /// button share one `TabGroup`; tabbing all the way around wraps back
    /// to the first.
    #[test]
    fn tab_order_covers_every_buy_button_and_the_back_button() {
        let mut app = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        set_state(&mut app, GameState::Shop);

        let total_controls = app
            .world_mut()
            .query_filtered::<(), With<crate::ui_widgets::focus::Focusable>>()
            .iter(app.world())
            .count();
        assert!(
            total_controls > CATALOG.len(),
            "at least every catalog item plus Back"
        );

        let tab = |app: &mut App| -> Option<Entity> {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
            keys.clear();
            app.world()
                .resource::<crate::ui_widgets::focus::InputFocus>()
                .get()
        };

        let first = tab(&mut app).expect("first control focused");
        for _ in 1..total_controls {
            tab(&mut app);
        }
        assert_eq!(
            tab(&mut app),
            Some(first),
            "tab order wraps back to the first control after visiting every one"
        );
    }

    #[test]
    fn pressing_an_unaffordable_buy_button_changes_nothing() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        press_item_button(&mut app, ItemId::Palos); // 150 > 50

        assert_eq!(wallet(&app), 50, "no debit, never a negative wallet");
        assert!(owned(&app).0.is_empty());
        assert_eq!(equipped(&app, Slot::Weapon), None);
    }

    #[test]
    fn equipping_an_owned_item_swaps_the_slot_without_a_debit() {
        let mut app = test_app();
        app.insert_resource(Wallet(1000));
        set_state(&mut app, GameState::Shop);

        press_item_button(&mut app, ItemId::BataCiobaneasca); // 20 g, +3 dmg
        assert!(texts(&mut app).contains(&"Atac: 9".to_string()), "6 + 3");

        press_item_button(&mut app, ItemId::Palos); // 150 g, +10 dmg
        assert_eq!(wallet(&app), 1000 - 20 - 150);
        assert_eq!(equipped(&app, Slot::Weapon), Some(ItemId::Palos));
        assert!(texts(&mut app).contains(&"Atac: 16".to_string()), "6 + 10");
        assert_eq!(
            item_button_label(&mut app, ItemId::BataCiobaneasca),
            "Echipează",
            "owned but not equipped"
        );
        assert!(!is_disabled(&mut app, ItemId::BataCiobaneasca));

        press_item_button(&mut app, ItemId::BataCiobaneasca);
        assert_eq!(
            equipped(&app, Slot::Weapon),
            Some(ItemId::BataCiobaneasca),
            "equipping swaps the slot back"
        );
        assert_eq!(wallet(&app), 830, "equipping owned gear is free");
        assert_eq!(item_button_label(&mut app, ItemId::Palos), "Echipează");
        assert_eq!(
            item_button_label(&mut app, ItemId::BataCiobaneasca),
            "Echipat"
        );
        assert!(
            texts(&mut app).contains(&"Armă / mână: Bâtă ciobănească".to_string()),
            "loadout preview tracks slot swaps"
        );
    }

    #[test]
    fn reentering_the_shop_shows_the_saved_states() {
        let mut app = test_app();
        app.insert_resource(Wallet(1000));
        set_state(&mut app, GameState::Shop);
        press_item_button(&mut app, ItemId::BataCiobaneasca);
        press_item_button(&mut app, ItemId::Palos);

        set_state(&mut app, GameState::Fight);
        set_state(&mut app, GameState::Shop);

        assert_eq!(item_button_label(&mut app, ItemId::Palos), "Echipat");
        assert_eq!(
            item_button_label(&mut app, ItemId::BataCiobaneasca),
            "Echipează"
        );
        assert!(
            texts(&mut app).contains(&"Pungă: 830 galbeni".to_string()),
            "the balance survives the fight"
        );
    }

    #[test]
    fn inapoi_in_arena_returns_to_the_fight_and_cleans_up() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let back = find_button(&mut app, ShopAction::BackToArena);
        press(&mut app, back);

        assert_eq!(state(&app), GameState::Fight);
        assert_eq!(count::<ShopScreen>(&mut app), 0, "root despawned");
        assert_eq!(count::<Button>(&mut app), 0, "buttons despawned");
        assert_eq!(count::<Text>(&mut app), 0, "labels despawned");
    }

    #[test]
    fn the_loadout_dresses_the_next_fights_player_fighter() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.add_plugins((ArenaPlugin, ShopPlugin));
        app.insert_resource(player_character());
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::ScutFerecat);
        app.insert_resource(PlayerEquipment(loadout.clone()));
        app.update();

        set_state(&mut app, GameState::Fight);
        app.update();

        let player_equipment = app
            .world_mut()
            .query_filtered::<&Equipment, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        assert_eq!(
            *player_equipment, loadout,
            "the fighter wears the bought gear"
        );
        assert_eq!(player_equipment.total_damage_bonus(), 10);
        assert_eq!(player_equipment.total_armor(), 3);
        let enemy_equipment = app
            .world_mut()
            .query_filtered::<&Equipment, With<crate::character::EnemyFighter>>()
            .single(app.world())
            .expect("enemy fighter exists");
        assert_eq!(
            *enemy_equipment,
            Equipment::default(),
            "only the player is dressed"
        );
    }

    #[test]
    fn equipment_changes_refresh_a_live_arena_player_fighter() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.add_plugins((ArenaPlugin, ShopPlugin));
        app.insert_resource(player_character());
        app.update();

        set_state(&mut app, GameState::Fight);
        app.update();

        let player = app
            .world_mut()
            .query_filtered::<Entity, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        assert!(
            gear_layers_for_owner(&mut app, player).is_empty(),
            "player starts bare before the loadout changes"
        );

        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        loadout.equip(ItemId::ScutFerecat);
        app.insert_resource(PlayerEquipment(loadout.clone()));
        app.update();

        let player_equipment = app
            .world_mut()
            .query_filtered::<&Equipment, With<PlayerFighter>>()
            .single(app.world())
            .expect("player fighter exists");
        assert_eq!(*player_equipment, loadout);
        assert_eq!(
            gear_layers_for_owner(&mut app, player),
            vec![
                (ItemId::Palos, Slot::Weapon, CutoutPartKind::HandFront),
                (
                    ItemId::ScutFerecat,
                    Slot::Shield,
                    CutoutPartKind::ForearmBack
                ),
            ]
        );
    }

    #[test]
    fn a_run_reset_clears_the_purchases_with_the_wallet() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            ProgressionPlugin,
            ShopPlugin,
        ));
        app.add_message::<CombatLogEvent>();
        app.update();
        app.insert_resource(Wallet(123));
        app.insert_resource(OwnedItems(HashSet::from([ItemId::Palos])));
        let mut loadout = Equipment::default();
        loadout.equip(ItemId::Palos);
        app.insert_resource(PlayerEquipment(loadout));
        set_state(&mut app, GameState::GameOver);

        let button = app
            .world_mut()
            .query_filtered::<Entity, (With<Button>, With<GameOverAction>)>()
            .single(app.world())
            .expect("back-to-menu button exists");
        press(&mut app, button);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(*app.world().resource::<Wallet>(), Wallet::default());
        assert_eq!(
            *app.world().resource::<OwnedItems>(),
            OwnedItems::default(),
            "owned items reset with the run"
        );
        assert_eq!(
            *app.world().resource::<PlayerEquipment>(),
            PlayerEquipment::default(),
            "the loadout resets with the run"
        );
    }
}
