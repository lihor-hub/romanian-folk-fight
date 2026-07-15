//! Save/load of a run (#21): a versioned run snapshot ([`snapshot::SaveGame`])
//! JSON snapshot of every run-scoped resource, persisted to `localStorage`
//! (key [`STORAGE_KEY`]) on wasm and to
//! `dirs::data_dir()/romanian-folk-fight/save.json` on native.
//!
//! #193 split this module in two: [`snapshot`] owns the schema, version
//! envelope, migration, and run-field capture/restore/reset contract; this
//! module owns the autosave/delete wiring below plus, in turn, [`storage`]
//! (#201) — *where the JSON physically lives*: the [`SaveBackend`] trait,
//! its native/web/in-memory implementations (native writes are a
//! same-directory temp file plus an atomic rename, never a torn in-place
//! write — see [`storage`]'s module docs), and the shared typed load
//! outcome ([`storage::SnapshotLoad`]) both platforms report. None of the
//! three knows the others' concerns: `snapshot` never touches a filesystem
//! or `localStorage`, `storage` never inspects a payload's fields (it always
//! goes through [`snapshot::SaveGame::load`]), and this top-level module only
//! wires autosave requests and the game-over delete — the main menu
//! (`crate::menu`) is what turns [`storage::SnapshotLoad`] into a
//! **Continuă** button or a Romanian recovery action.
//!
//! Autosave points write a [`SaveRequested`] message (after the victory
//! payout, after a level-up allocation confirm, after every shop purchase or
//! equip); [`persist_on_request`] turns it into a stored snapshot. A run is
//! one life: game over deletes the save. The main menu's **Continuă** button
//! loads the snapshot back into the resources and jumps straight into
//! [`GameState::Fight`]. Corrupt, partially-written, or unsupported-version
//! saves never panic (see [`snapshot::SaveGame::load`]); the menu offers a
//! recovery action instead of the game silently discarding them (#201).

pub mod snapshot;
pub mod storage;

pub use snapshot::{CURRENT_VERSION, ResumeDestination, SaveGame};
pub use storage::{
    SaveBackend, SaveStore, SnapshotLoad, load_save, load_save_outcome, platform_backend,
};

#[cfg(test)]
pub(crate) use storage::MemoryBackend;

use bevy::prelude::*;

use crate::core::GameState;
use crate::creation::PlayerCharacter;
use crate::progression::{Level, LifetimeEarnings, Wallet};
use crate::roster::LadderProgress;
use crate::shop::{OwnedItems, PlayerEquipment};

/// The `localStorage` key of the wasm backend. The `_v1` names the storage
/// bucket, not the schema version — the schema version lives inside the
/// payload's `"version"` field (see [`snapshot::CURRENT_VERSION`]), which is
/// what lets [`snapshot::SaveGame::from_json`] migrate in place without ever
/// needing a new storage location.
pub const STORAGE_KEY: &str = "rff_save_v1";

/// Fired by the autosave hooks (victory payout, level-up confirm, shop
/// purchase/equip); [`persist_on_request`] snapshots the run in response.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct SaveRequested;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveStore>()
            .add_message::<SaveRequested>()
            // Ordered after `FlowIntentEmission` (#155) so that when a menu
            // or creation system both queues resource-inserting `Commands`
            // and requests a save in the same frame (e.g. confirming a new
            // hero), those commands are guaranteed flushed — Bevy
            // auto-inserts the `apply_deferred` sync point for an explicit
            // ordering edge — before this system reads the run resources.
            // Without this, the two systems are ambiguous and may run in
            // either order.
            .add_systems(
                Update,
                persist_on_request.after(crate::flow::FlowIntentEmission),
            )
            .add_systems(OnEnter(GameState::GameOver), delete_save);
    }
}

/// Turns pending [`SaveRequested`] messages into one stored snapshot of the
/// run. Skips (with a warning) if any run resource is missing — autosaves
/// only fire mid-run, where all of them exist.
// A Bevy system: each parameter is a distinct ECS handle for one of the
// seven run-scoped resources being snapshotted (see `snapshot`'s ownership
// contract).
#[allow(clippy::too_many_arguments)]
fn persist_on_request(
    mut requests: MessageReader<SaveRequested>,
    store: Res<SaveStore>,
    player: Option<Res<PlayerCharacter>>,
    level: Option<Res<Level>>,
    wallet: Option<Res<Wallet>>,
    lifetime_earnings: Option<Res<LifetimeEarnings>>,
    owned: Option<Res<OwnedItems>>,
    equipment: Option<Res<PlayerEquipment>>,
    ladder: Option<Res<LadderProgress>>,
) {
    if requests.is_empty() {
        return;
    }
    requests.clear();
    let (
        Some(player),
        Some(level),
        Some(wallet),
        Some(lifetime_earnings),
        Some(owned),
        Some(equipment),
        Some(ladder),
    ) = (
        player,
        level,
        wallet,
        lifetime_earnings,
        owned,
        equipment,
        ladder,
    )
    else {
        warn!("autosave requested without a full run in place; nothing saved");
        return;
    };
    let save = SaveGame::capture(
        &player,
        &level,
        &wallet,
        &lifetime_earnings,
        &owned,
        &equipment,
        &ladder,
    );
    match save.to_json() {
        Some(json) => store.store(&json),
        None => warn!("could not serialize the save; nothing saved"),
    }
}

/// A run is one life (Sword & Sandals style): reaching game over deletes the
/// save, so **Continuă** goes back to disabled.
fn delete_save(store: Res<SaveStore>) {
    store.clear();
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::character::{Attributes, PlayerAppearance};
    use crate::combat::{CombatLogEvent, CombatSide};
    use crate::core::CorePlugin;
    use crate::flow::FlowPlugin;
    use crate::items::ItemId;
    use crate::progression::{
        FightOutcome, ProgressionPlugin, STARTING_GALBENI,
        result_ui::{AllocateAction, GameOverAction},
    };
    use crate::roster::RosterPlugin;
    use crate::save::snapshot::tests::sample_save;
    use crate::shop::{ShopAction, ShopPlugin};

    // `load_save`/`load_save_outcome` (and the `SnapshotLoad` classification
    // they're built on) are storage-layer concerns now split into
    // `save::storage` (#201) -- see `cargo test save::storage --lib` for
    // their coverage, including the moved
    // `load_save_clears_an_invalid_store`/
    // `load_save_returns_a_valid_snapshot_and_keeps_it_stored` tests.

    // --- autosave and delete flows ---

    /// Headless app with the whole run flow (progression, roster, shop) and
    /// the save plugin over an in-memory store.
    fn test_app() -> (App, Arc<Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            ProgressionPlugin,
            RosterPlugin,
            ShopPlugin,
            SavePlugin,
        ));
        app.add_message::<CombatLogEvent>();
        let (store, cell) = SaveStore::in_memory();
        app.insert_resource(store);
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes {
                putere: 4,
                agilitate: 2,
                vitalitate: 4,
                noroc: 3,
            },
            appearance: PlayerAppearance {
                skin_tone: crate::character::SkinTone::Deep,
                build: crate::character::BodyBuild::Balanced,
                hair: crate::character::HairStyle::Braided,
                accent: crate::character::AccentColor::Storm,
            },
        });
        app.update();
        (app, cell)
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn stored_save(cell: &Arc<Mutex<Option<String>>>) -> Option<SaveGame> {
        cell.lock()
            .expect("test store lock")
            .as_deref()
            .and_then(SaveGame::from_json)
    }

    /// Presses `button`: handler on the first update, transitions and the
    /// persist system on the second.
    fn press(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    #[test]
    fn the_victory_payout_autosaves_the_credited_run() {
        let (mut app, cell) = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        app.update(); // persist system consumes the request

        let save = stored_save(&cell).expect("victory writes a save");
        assert_eq!(save.wallet, STARTING_GALBENI + 35, "saved after the credit");
        assert_eq!(save.xp, 20, "saved after the XP award");
        assert_eq!(
            save.ladder_progress, 1,
            "saved after the ladder advanced to the next opponent"
        );
    }

    #[test]
    fn confirming_a_level_up_allocation_autosaves_the_new_build() {
        let (mut app, cell) = test_app();
        app.insert_resource(Level {
            level: 2,
            xp: 0,
            unspent_points: 2,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        cell.lock().expect("test store lock").take(); // drop the victory autosave

        let increase = app
            .world_mut()
            .query_filtered::<(Entity, &AllocateAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == AllocateAction::Increase(crate::character::AttributeKind::Putere))
            .map(|(entity, _)| entity)
            .expect("allocation button exists");
        press(&mut app, increase);
        assert_eq!(
            stored_save(&cell),
            None,
            "allocation clicks alone do not save"
        );

        let confirm = app
            .world_mut()
            .query_filtered::<(Entity, &AllocateAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == AllocateAction::Confirm)
            .map(|(entity, _)| entity)
            .expect("confirm button exists");
        press(&mut app, confirm);

        let save = stored_save(&cell).expect("confirm writes a save");
        assert_eq!(save.attrs.putere, 5, "the allocated point is saved");
        assert_eq!(save.unspent_points, 1, "the leftover point is saved");
    }

    #[test]
    fn a_purchase_and_an_equip_swap_both_autosave() {
        let (mut app, cell) = test_app();
        app.insert_resource(Wallet(1000));
        set_state(&mut app, GameState::Shop);

        let item_button = |app: &mut App, id: ItemId| {
            app.world_mut()
                .query_filtered::<(Entity, &ShopAction), With<Button>>()
                .iter(app.world())
                .find(|&(_, &a)| a == ShopAction::Item(id))
                .map(|(entity, _)| entity)
                .expect("item button exists")
        };

        let button = item_button(&mut app, ItemId::BataCiobaneasca);
        press(&mut app, button);
        let save = stored_save(&cell).expect("a purchase writes a save");
        assert_eq!(save.wallet, 980, "saved after the debit");
        assert_eq!(save.owned_items, vec!["BataCiobaneasca"]);
        assert_eq!(save.equipped, vec!["BataCiobaneasca"], "auto-equip saved");

        let button = item_button(&mut app, ItemId::Palos);
        press(&mut app, button);
        let save = stored_save(&cell).expect("the second purchase saves too");
        assert_eq!(save.equipped, vec!["Palos"]);

        // Swapping back to owned gear (no purchase) also persists.
        let button = item_button(&mut app, ItemId::BataCiobaneasca);
        press(&mut app, button);
        let save = stored_save(&cell).expect("an equip swap writes a save");
        assert_eq!(save.equipped, vec!["BataCiobaneasca"]);
        assert_eq!(save.wallet, 830, "equipping owned gear is free");
    }

    #[test]
    fn a_failed_purchase_does_not_save() {
        let (mut app, cell) = test_app();
        set_state(&mut app, GameState::Shop);

        let button = app
            .world_mut()
            .query_filtered::<(Entity, &ShopAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == ShopAction::Item(ItemId::Palos))
            .map(|(entity, _)| entity)
            .expect("item button exists");
        press(&mut app, button); // 150 > 50: rejected

        assert_eq!(stored_save(&cell), None, "nothing bought, nothing saved");
    }

    #[test]
    fn game_over_deletes_the_save() {
        let (mut app, cell) = test_app();
        let json = sample_save().to_json().expect("plain data serializes");
        app.world_mut().resource::<SaveStore>().store(&json);
        assert!(stored_save(&cell).is_some());

        set_state(&mut app, GameState::GameOver);

        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "one life per run: game over deletes the save"
        );

        // Leaving game over resets the run without resurrecting the save.
        let button = app
            .world_mut()
            .query_filtered::<Entity, (With<Button>, With<GameOverAction>)>()
            .single(app.world())
            .expect("back-to-menu button exists");
        press(&mut app, button);
        assert_eq!(*cell.lock().expect("test store lock"), None);
    }
}
