//! The shop screen ("Prăvălia lui Moș Pintea") for `GameState::Shop`: the
//! player spends galbeni on catalog gear between fights.
//!
//! Purchases live in the run-scoped [`OwnedItems`] set and the equipped
//! loadout in [`PlayerEquipment`]; both reset with the run like `Wallet`.
//! The pure [`try_buy`] holds the purchase rules so the UI systems stay thin.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::character::{Attributes, PlayerFighter, stats};
use crate::core::{GameState, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::items::{CATALOG, Equipment, Item, ItemId, Slot};
use crate::menu::{
    BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, DisabledButton,
    NIGHT_BLACK, TEXT_DISABLED,
};
use crate::progression::Wallet;
use crate::ui_widgets::wide_button;

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
    /// The label of one item's buy/equip button.
    ItemButton(ItemId),
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

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        // `Wallet` normally comes from `ProgressionPlugin`; init here too so
        // the shop is self-contained (idempotent when both plugins run).
        app.init_resource::<Wallet>()
            .init_resource::<OwnedItems>()
            .init_resource::<PlayerEquipment>()
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
                )
                    .chain()
                    .run_if(in_state(GameState::Shop)),
            )
            .add_systems(OnExit(GameState::Shop), despawn_screen::<ShopScreen>)
            .add_systems(
                Update,
                dress_player_fighter.run_if(in_state(GameState::Fight)),
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
fn player_attributes(player: Option<Res<PlayerCharacter>>) -> Attributes {
    match player {
        Some(player) => player.attributes,
        None => {
            warn!("in GameState::Shop without a PlayerCharacter; showing the default build");
            Attributes::default()
        }
    }
}

/// Spawns the shop screen: header with the wallet top-right, the catalog
/// grouped by slot, the live stat summary, and the back-to-arena button.
fn spawn_shop_screen(
    mut commands: Commands,
    wallet: Res<Wallet>,
    owned: Res<OwnedItems>,
    equipment: Res<PlayerEquipment>,
    player: Option<Res<PlayerCharacter>>,
) {
    let attributes = player_attributes(player);
    commands
        .spawn((
            ShopScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(NIGHT_BLACK),
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
                        TextFont {
                            font_size: FontSize::Px(32.0),
                            ..default()
                        },
                        TextColor(CREAM),
                    ));
                    header.spawn((line_text(wallet_text(&wallet), 20.0), ShopLabel::Wallet));
                });
            // The catalog, grouped by slot.
            for slot in Slot::ALL {
                parent.spawn((
                    line_text(slot_label(slot).to_string(), 18.0),
                    Node {
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                ));
                for item in CATALOG.iter().filter(|item| item.slot == slot) {
                    let state = ItemButtonState::of(item.id, &wallet, &owned, &equipment);
                    spawn_item_row(parent, item, state);
                }
            }
            // Live stat summary: purchases visibly matter.
            parent
                .spawn(Node {
                    column_gap: Val::Px(24.0),
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|panel| {
                    panel.spawn((
                        line_text(attack_text(&attributes, &equipment), 20.0),
                        ShopLabel::Attack,
                    ));
                    panel.spawn((line_text(armor_text(&equipment), 20.0), ShopLabel::Armor));
                    panel.spawn((line_text(health_text(&attributes), 20.0), ShopLabel::Health));
                });
            parent.spawn((wide_button("Înapoi în arenă"), ShopAction::BackToArena));
        });
}

/// A cream text line of the given font size.
fn line_text(label: String, font_size: f32) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(CREAM),
    )
}

/// One catalog row: name, stat, price, and the buy/equip/equipped button.
fn spawn_item_row(parent: &mut ChildSpawnerCommands, item: &Item, state: ItemButtonState) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((column(220.0), line_text(item.name.to_string(), 16.0)));
            row.spawn((column(90.0), line_text(stat_text(item), 16.0)));
            row.spawn((
                column(90.0),
                line_text(format!("{} galbeni", item.price), 16.0),
            ));
            let mut button = row.spawn((
                Button,
                Node {
                    width: Val::Px(120.0),
                    height: Val::Px(26.0),
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
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
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
/// stale-looking button can never overdraw the wallet.
fn handle_shop_actions(
    interactions: Query<(&Interaction, &ShopAction), ChangedButton>,
    mut wallet: ResMut<Wallet>,
    mut owned: ResMut<OwnedItems>,
    mut equipment: ResMut<PlayerEquipment>,
    mut next_state: ResMut<NextState<GameState>>,
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
                    }
                    ItemButtonState::Buy => {
                        if try_buy(&mut wallet, &mut owned, id).is_ok() {
                            equipment.0.equip(id);
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
    let attributes = player_attributes(player);
    for (mut text, mut color, label) in &mut labels {
        let new = match *label {
            ShopLabel::Wallet => wallet_text(&wallet),
            ShopLabel::Attack => attack_text(&attributes, &equipment),
            ShopLabel::Armor => armor_text(&equipment),
            ShopLabel::Health => health_text(&attributes),
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
/// freshly spawned by the arena, so purchases show up in the next fight's
/// damage/armor numbers without the shop touching arena code. `Added` keeps
/// it a one-shot per spawn.
fn dress_player_fighter(
    loadout: Res<PlayerEquipment>,
    mut fighters: Query<&mut Equipment, (With<PlayerFighter>, Added<Equipment>)>,
) {
    for mut equipment in &mut fighters {
        *equipment = loadout.0.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::combat::CombatLogEvent;
    use crate::core::CorePlugin;
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
        assert!(texts.contains(&"Înapoi în arenă".to_string()), "{texts:?}");
        assert_eq!(
            count::<Button>(&mut app),
            CATALOG.len() + 1,
            "one button per item plus the back button"
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
