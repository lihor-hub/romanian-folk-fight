//! The shop screen ("Prăvălia lui Moș Pintea") for `GameState::Shop`: the
//! player spends galbeni on catalog gear between fights.
//!
//! Purchases live in the run-scoped [`OwnedItems`] set and the equipped
//! loadout in [`PlayerEquipment`]; both reset with the run like `Wallet`.
//! The pure [`try_buy`] holds the purchase rules so the UI systems stay thin.

use std::collections::HashSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::character::{Attributes, PlayerAppearance, PlayerFighter, stats};
use crate::core::{GameState, UiFont, ViewportInfo, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::cutout::{CutoutRig, human_template_for, spawn_cutout_rig_with_gear};
use crate::items::{CATALOG, Equipment, Item, ItemId, Slot};
use crate::menu::DisabledButton;
use crate::progression::Wallet;
use crate::save::SaveRequested;
use crate::theme::{
    ARENA_BROWN, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, GOLD,
    MIN_TOUCH_TARGET, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
use crate::ui_widgets::wide_button;

const SHOP_ROOT_PADDING: f32 = 12.0;
#[cfg(test)]
const SHOP_TARGET_WIDTH: f32 = 800.0;
const SHOP_BODY_WIDTH: f32 = 760.0;
const SHOP_BODY_GAP: f32 = 12.0;
const SHOP_CATALOG_WIDTH: f32 = 430.0;
const SHOP_PREVIEW_STAGE_WIDTH: f32 = 318.0;
const SHOP_PANEL_HEIGHT: f32 = 450.0;
const SHOP_PREVIEW_FRAME_HEIGHT: f32 = 216.0;
const SHOP_PREVIEW_SCALE: f32 = 0.68;
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
            .add_plugins(crate::ui_widgets::ScrollInputPlugin)
            .add_message::<SaveRequested>()
            .add_systems(PreStartup, load_shop_icons)
            .add_systems(OnEnter(GameState::Shop), spawn_shop_screen)
            .add_systems(
                Update,
                (
                    handle_shop_actions,
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
                    .run_if(resource_changed::<ViewportInfo>)
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
            BackgroundColor(ARENA_BROWN),
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            // Header row: title on the left, wallet balance top-right.
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(8.0)),
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
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::FlexStart,
                    column_gap: Val::Px(SHOP_BODY_GAP),
                    row_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|body| {
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
                                let state =
                                    ItemButtonState::of(item.id, &wallet, &owned, &equipment);
                                spawn_item_row(catalog, item, state, ui_font, panel_texture);
                            }
                        }
                    });

                    body.spawn((
                        panel_bundle(
                            panel_texture,
                            Node {
                                width: Val::Px(SHOP_PREVIEW_STAGE_WIDTH),
                                max_width: Val::Percent(100.0),
                                min_height: Val::Px(SHOP_PANEL_HEIGHT),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(8.0),
                                padding: UiRect::all(Val::Px(14.0)),
                                ..default()
                            },
                        ),
                        BackgroundColor(PANEL_LINEN),
                        ShopLayoutRole::PreviewStage,
                    ))
                    .with_children(|preview_panel| {
                        preview_panel.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(SHOP_PREVIEW_FRAME_HEIGHT),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(WALNUT),
                            BorderColor::all(GOLD),
                        ));
                        spawn_loadout_preview(preview_panel, &equipment, ui_font, icons);
                        spawn_stat_strip(preview_panel, &attributes, &equipment, ui_font);
                    });
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
        viewport.width,
        assets.asset_server.as_deref(),
    );
}

fn spawn_shop_preview(
    commands: &mut Commands,
    equipment: &Equipment,
    appearance: PlayerAppearance,
    viewport_width: f32,
    asset_server: Option<&AssetServer>,
) {
    let preview = commands
        .spawn((
            ShopScreen,
            ShopPreview,
            shop_preview_transform_for_width(viewport_width),
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

fn shop_preview_stage_center_x() -> f32 {
    -SHOP_BODY_WIDTH / 2.0 + SHOP_CATALOG_WIDTH + SHOP_BODY_GAP + SHOP_PREVIEW_STAGE_WIDTH / 2.0
}

fn shop_preview_x_for_width(viewport_width: f32) -> f32 {
    let usable_width = viewport_width - SHOP_ROOT_PADDING * 2.0;
    let desktop_width = SHOP_CATALOG_WIDTH + SHOP_BODY_GAP + SHOP_PREVIEW_STAGE_WIDTH;
    if usable_width >= desktop_width {
        shop_preview_stage_center_x()
    } else {
        0.0
    }
}

fn shop_preview_transform_for_width(viewport_width: f32) -> Transform {
    Transform::from_xyz(
        shop_preview_x_for_width(viewport_width),
        SHOP_PREVIEW_Y,
        SHOP_PREVIEW_Z,
    )
    .with_scale(Vec3::splat(SHOP_PREVIEW_SCALE))
}

#[cfg(test)]
fn shop_layout_fits_width(viewport_width: f32) -> bool {
    let usable_width = viewport_width - SHOP_ROOT_PADDING * 2.0;
    let desktop_width = SHOP_CATALOG_WIDTH + SHOP_BODY_GAP + SHOP_PREVIEW_STAGE_WIDTH;
    desktop_width <= SHOP_BODY_WIDTH
        && SHOP_CATALOG_WIDTH.min(usable_width) <= usable_width
        && SHOP_PREVIEW_STAGE_WIDTH.min(usable_width) <= usable_width
}

fn update_shop_preview_transform(
    viewport: Res<ViewportInfo>,
    mut previews: Query<&mut Transform, With<ShopPreview>>,
) {
    for mut transform in &mut previews {
        transform.translation.x = shop_preview_x_for_width(viewport.width);
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

/// Runs the [`ShopAction`] of whichever shop button was pressed: buys (via
/// [`try_buy`]) and auto-equips, equips owned items, or leaves for the
/// arena. State is re-derived from the resources on every press, so a
/// stale-looking button can never overdraw the wallet. Every successful
/// purchase or equip swap autosaves the run (see [`crate::save`]).
fn handle_shop_actions(
    interactions: Query<(&Interaction, &ShopAction), ChangedButton>,
    mut wallet: ResMut<Wallet>,
    mut owned: ResMut<OwnedItems>,
    mut equipment: ResMut<PlayerEquipment>,
    mut next_state: ResMut<NextState<GameState>>,
    mut save_requests: MessageWriter<SaveRequested>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            ShopAction::BackToArena => next_state.set(GameState::Fight),
            ShopAction::Item(id) => {
                match ItemButtonState::of(id, &wallet, &owned, &equipment) {
                    ItemButtonState::Equip => {
                        equipment.0.equip(id);
                        save_requests.write(SaveRequested);
                    }
                    ItemButtonState::Buy => {
                        if try_buy(&mut wallet, &mut owned, id).is_ok() {
                            equipment.0.equip(id);
                            save_requests.write(SaveRequested);
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
    use crate::cutout::{CutoutPartKind, CutoutPartMarker, GearVisualLayer};
    use crate::progression::{ProgressionPlugin, STARTING_GALBENI, result_ui::GameOverAction};
    use bevy::state::app::StatesPlugin;

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
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ShopPlugin));
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

    fn gear_layers_for_owner(app: &mut App, owner: Entity) -> Vec<(ItemId, Slot, CutoutPartKind)> {
        let part_info: Vec<(Entity, Entity, CutoutPartKind)> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker, &ChildOf)>()
            .iter(app.world())
            .map(|(part, marker, child_of)| (part, child_of.parent(), marker.kind))
            .collect();
        let mut layers: Vec<(ItemId, Slot, CutoutPartKind)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let (_, part_owner, kind) = part_info
                    .iter()
                    .find(|(part, _, _)| *part == child_of.parent())?;
                (*part_owner == owner).then_some((layer.item, layer.slot, *kind))
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

    #[test]
    fn shop_preview_rig_is_centered_from_stage_constants() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);

        let transform = app
            .world_mut()
            .query_filtered::<&Transform, With<ShopPreview>>()
            .single(app.world())
            .expect("shop preview transform exists");
        let expected = shop_preview_transform_for_width(SHOP_TARGET_WIDTH);
        assert_eq!(transform.translation, expected.translation);
        assert_eq!(transform.scale, expected.scale);
        assert!((transform.translation.x - shop_preview_stage_center_x()).abs() < f32::EPSILON);
        assert_eq!(
            shop_preview_x_for_width(375.0),
            0.0,
            "wrapped shop preview stage is centered on its own row"
        );
        assert!(transform.translation.x.abs() <= SHOP_BODY_WIDTH / 2.0);
        assert!(transform.translation.y.abs() <= SHOP_PANEL_HEIGHT / 2.0);
    }

    #[test]
    fn shop_preview_starts_centered_when_entering_narrow_viewport() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<ViewportInfo>()
            .set_if_neq(ViewportInfo {
                width: 375.0,
                height: 812.0,
                is_mobile: true,
            });
        set_state(&mut app, GameState::Shop);

        let transform = app
            .world_mut()
            .query_filtered::<&Transform, With<ShopPreview>>()
            .single(app.world())
            .expect("shop preview transform exists");
        assert_eq!(transform.translation.x, 0.0);
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
