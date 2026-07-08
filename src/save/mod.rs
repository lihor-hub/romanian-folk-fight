//! Save/load of a run (#21): a versioned [`SaveGame`] JSON snapshot of every
//! run-scoped resource, persisted to `localStorage` (key [`STORAGE_KEY`]) on
//! wasm and to `dirs::data_dir()/romanian-folk-fight/save.json` on native.
//!
//! Autosave points write a [`SaveRequested`] message (after the victory
//! payout, after a level-up allocation confirm, after every shop purchase or
//! equip); [`persist_on_request`] turns it into a stored snapshot. A run is
//! one life: game over deletes the save. The main menu's **Continuă** button
//! loads the snapshot back into the resources and jumps straight into
//! [`GameState::Fight`]. Corrupt or version-mismatched saves are discarded
//! (never a panic) and the store is cleared.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::{Attributes, PlayerAppearance};
use crate::core::GameState;
use crate::creation::PlayerCharacter;
use crate::items::{Equipment, ItemId, Slot};
use crate::progression::{Level, Wallet};
use crate::roster::LadderProgress;
use crate::shop::{OwnedItems, PlayerEquipment};

/// The version written into every save; loads of any other version are
/// discarded. Additive fields can still default safely inside a version.
pub const SAVE_VERSION: u32 = 1;

/// The `localStorage` key of the wasm backend.
pub const STORAGE_KEY: &str = "rff_save_v1";

/// Fired by the autosave hooks (victory payout, level-up confirm, shop
/// purchase/equip); [`persist_on_request`] snapshots the run in response.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct SaveRequested;

/// Serde mirror of [`Attributes`]; the character model stays serde-free.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedAttributes {
    pub putere: u32,
    pub agilitate: u32,
    pub vitalitate: u32,
    pub noroc: u32,
}

impl From<Attributes> for SavedAttributes {
    fn from(attrs: Attributes) -> Self {
        Self {
            putere: attrs.putere,
            agilitate: attrs.agilitate,
            vitalitate: attrs.vitalitate,
            noroc: attrs.noroc,
        }
    }
}

impl From<SavedAttributes> for Attributes {
    fn from(attrs: SavedAttributes) -> Self {
        Self {
            putere: attrs.putere,
            agilitate: attrs.agilitate,
            vitalitate: attrs.vitalitate,
            noroc: attrs.noroc,
        }
    }
}

/// The stable save name of a catalog item (its `ItemId` variant name).
fn item_name(id: ItemId) -> String {
    format!("{id:?}")
}

/// Resolves a save name back to its catalog id; `None` for unknown names
/// (which invalidate the whole save — see [`SaveGame::from_json`]).
fn parse_item(name: &str) -> Option<ItemId> {
    ItemId::ALL.into_iter().find(|id| item_name(*id) == name)
}

/// One full run snapshot, mirroring every run-scoped resource: the confirmed
/// character, the experience state, the wallet, the shop purchases and
/// loadout, and the ladder position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SaveGame {
    /// Always [`SAVE_VERSION`]; any other value discards the save on load.
    pub version: u32,
    /// [`PlayerCharacter::name`].
    pub name: String,
    /// [`PlayerCharacter::attributes`].
    pub attrs: SavedAttributes,
    /// [`PlayerCharacter::appearance`]. Missing on older v1 saves, where it
    /// defaults to the current project baseline.
    #[serde(default)]
    pub appearance: PlayerAppearance,
    /// [`Level::level`].
    pub level: u32,
    /// [`Level::xp`].
    pub xp: u32,
    /// [`Level::unspent_points`].
    pub unspent_points: u32,
    /// [`Wallet`] balance in galbeni.
    pub wallet: u32,
    /// [`OwnedItems`] as sorted item names, for deterministic JSON.
    pub owned_items: Vec<String>,
    /// [`PlayerEquipment`] as item names in [`Slot::ALL`] order.
    pub equipped: Vec<String>,
    /// [`LadderProgress`]: the 0-based index of the next opponent.
    pub ladder_progress: usize,
    /// The 1-based ladder lap; derived from `ladder_progress`, stored for
    /// human-readable saves.
    pub lap: u32,
}

impl SaveGame {
    /// Snapshots the current run from its resources.
    pub fn capture(
        player: &PlayerCharacter,
        level: &Level,
        wallet: &Wallet,
        owned: &OwnedItems,
        equipment: &PlayerEquipment,
        ladder: &LadderProgress,
    ) -> Self {
        let mut owned_items: Vec<String> = owned.0.iter().map(|id| item_name(*id)).collect();
        owned_items.sort();
        let equipped = Slot::ALL
            .into_iter()
            .filter_map(|slot| equipment.0.equipped(slot))
            .map(item_name)
            .collect();
        Self {
            version: SAVE_VERSION,
            name: player.name.clone(),
            attrs: player.attributes.into(),
            appearance: player.appearance,
            level: level.level,
            xp: level.xp,
            unspent_points: level.unspent_points,
            wallet: wallet.0,
            owned_items,
            equipped,
            ladder_progress: ladder.0,
            lap: ladder.lap(),
        }
    }

    /// The snapshot as JSON; `None` only if serialization itself fails
    /// (which plain data like this never does — handled instead of unwrapped
    /// to keep runtime code panic-free).
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    /// Parses and validates a snapshot: corrupt JSON, a version other than
    /// [`SAVE_VERSION`], or an item name missing from the catalog all yield
    /// `None` — never a panic.
    pub fn from_json(json: &str) -> Option<Self> {
        let save: Self = serde_json::from_str(json).ok()?;
        if save.version != SAVE_VERSION {
            warn!(
                "save version {} does not match {}; discarding",
                save.version, SAVE_VERSION
            );
            return None;
        }
        if let Some(unknown) = save
            .owned_items
            .iter()
            .chain(&save.equipped)
            .find(|name| parse_item(name).is_none())
        {
            warn!("save references unknown item {unknown:?}; discarding");
            return None;
        }
        Some(save)
    }

    /// The saved [`PlayerCharacter`].
    pub fn player_character(&self) -> PlayerCharacter {
        PlayerCharacter {
            name: self.name.clone(),
            attributes: self.attrs.into(),
            appearance: self.appearance,
        }
    }

    /// The saved [`Level`].
    pub fn level(&self) -> Level {
        Level {
            level: self.level,
            xp: self.xp,
            unspent_points: self.unspent_points,
        }
    }

    /// The saved [`Wallet`].
    pub fn wallet(&self) -> Wallet {
        Wallet(self.wallet)
    }

    /// The saved [`OwnedItems`]. Unknown names can't reach here (validated
    /// by [`Self::from_json`]); they are skipped defensively regardless.
    pub fn owned_items(&self) -> OwnedItems {
        OwnedItems(
            self.owned_items
                .iter()
                .filter_map(|name| parse_item(name))
                .collect(),
        )
    }

    /// The saved [`PlayerEquipment`].
    pub fn player_equipment(&self) -> PlayerEquipment {
        let mut equipment = Equipment::default();
        for id in self.equipped.iter().filter_map(|name| parse_item(name)) {
            equipment.equip(id);
        }
        PlayerEquipment(equipment)
    }

    /// The saved [`LadderProgress`].
    pub fn ladder_progress(&self) -> LadderProgress {
        LadderProgress(self.ladder_progress)
    }

    /// Restores every run resource from the snapshot; with the resources in
    /// place, entering [`GameState::Fight`] resumes the run exactly.
    pub fn restore(&self, commands: &mut Commands) {
        commands.insert_resource(self.player_character());
        commands.insert_resource(self.level());
        commands.insert_resource(self.wallet());
        commands.insert_resource(self.owned_items());
        commands.insert_resource(self.player_equipment());
        commands.insert_resource(self.ladder_progress());
    }
}

/// Where save JSON physically lives; one implementation per platform plus an
/// in-memory one for tests.
pub trait SaveBackend: Send + Sync + 'static {
    /// Writes the snapshot, replacing any previous one. Errors are logged,
    /// never panicked on.
    fn store(&self, json: &str);
    /// The stored snapshot, if any.
    fn load(&self) -> Option<String>;
    /// Deletes the stored snapshot, if any.
    fn clear(&self);
}

/// The save store of the running game: the platform backend by default
/// ([`Default`]), an in-memory one in tests.
#[derive(Resource)]
pub struct SaveStore(Box<dyn SaveBackend>);

impl SaveStore {
    /// A store over a specific backend (tests use the in-memory one).
    pub fn with_backend(backend: impl SaveBackend) -> Self {
        Self(Box::new(backend))
    }

    /// Writes the snapshot, replacing any previous one.
    pub fn store(&self, json: &str) {
        self.0.store(json);
    }

    /// The stored snapshot, if any.
    pub fn load(&self) -> Option<String> {
        self.0.load()
    }

    /// Deletes the stored snapshot, if any.
    pub fn clear(&self) {
        self.0.clear();
    }
}

impl Default for SaveStore {
    fn default() -> Self {
        Self(Box::new(platform_backend("save.json", STORAGE_KEY)))
    }
}

/// The platform backend at a custom location: a file named `file_name` under
/// the game's data directory on native, the `storage_key` entry of
/// `localStorage` on wasm. Lets other persisted blobs (e.g. the audio
/// settings, #30) reuse the same storage machinery under their own key.
#[cfg(not(target_arch = "wasm32"))]
pub fn platform_backend(file_name: &'static str, _storage_key: &'static str) -> impl SaveBackend {
    platform::PlatformBackend { file_name }
}

/// See the native `platform_backend`; on wasm the `storage_key` selects the
/// `localStorage` entry and the file name is unused.
#[cfg(target_arch = "wasm32")]
pub fn platform_backend(_file_name: &'static str, storage_key: &'static str) -> impl SaveBackend {
    platform::PlatformBackend { storage_key }
}

/// Loads and validates the stored save. A snapshot that fails validation
/// (corrupt JSON, version mismatch, unknown items) is cleared from the store
/// so the menu never re-reads a known-bad save.
pub fn load_save(store: &SaveStore) -> Option<SaveGame> {
    let json = store.load()?;
    let save = SaveGame::from_json(&json);
    if save.is_none() {
        warn!("discarding invalid save");
        store.clear();
    }
    save
}

/// Native backend: `dirs::data_dir()/romanian-folk-fight/save.json`.
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::path::PathBuf;

    use bevy::prelude::warn;

    use super::SaveBackend;

    pub struct PlatformBackend {
        /// File name under the game's data directory (e.g. `save.json`).
        pub file_name: &'static str,
    }

    impl PlatformBackend {
        /// The backing file path; `None` when the platform has no data
        /// directory.
        fn path(&self) -> Option<PathBuf> {
            Some(
                dirs::data_dir()?
                    .join("romanian-folk-fight")
                    .join(self.file_name),
            )
        }
    }

    impl SaveBackend for PlatformBackend {
        fn store(&self, json: &str) {
            let Some(path) = self.path() else {
                warn!("no platform data directory; save not written");
                return;
            };
            if let Some(parent) = path.parent()
                && let Err(err) = std::fs::create_dir_all(parent)
            {
                warn!("could not create save directory {parent:?}: {err}");
                return;
            }
            if let Err(err) = std::fs::write(&path, json) {
                warn!("could not write save file {path:?}: {err}");
            }
        }

        fn load(&self) -> Option<String> {
            std::fs::read_to_string(self.path()?).ok()
        }

        fn clear(&self) {
            if let Some(path) = self.path() {
                // A missing file is already "cleared"; other errors leave a
                // stale save behind, which the version/validation guard on
                // load keeps harmless.
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Web backend: `window.localStorage` under [`STORAGE_KEY`].
#[cfg(target_arch = "wasm32")]
mod platform {
    use bevy::prelude::warn;

    use super::SaveBackend;

    pub struct PlatformBackend {
        /// The `localStorage` key this backend reads and writes.
        pub storage_key: &'static str,
    }

    /// The window's local storage; `None` when unavailable (e.g. blocked by
    /// the browser).
    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    impl SaveBackend for PlatformBackend {
        fn store(&self, json: &str) {
            match local_storage() {
                Some(storage) => {
                    if storage.set_item(self.storage_key, json).is_err() {
                        warn!("could not write save to localStorage");
                    }
                }
                None => warn!("localStorage unavailable; save not written"),
            }
        }

        fn load(&self) -> Option<String> {
            local_storage()?.get_item(self.storage_key).ok().flatten()
        }

        fn clear(&self) {
            if let Some(storage) = local_storage() {
                let _ = storage.remove_item(self.storage_key);
            }
        }
    }
}

/// In-memory backend for tests: a shared cell the test inspects.
#[cfg(test)]
pub(crate) struct MemoryBackend(pub(crate) std::sync::Arc<std::sync::Mutex<Option<String>>>);

#[cfg(test)]
impl SaveBackend for MemoryBackend {
    fn store(&self, json: &str) {
        *self.0.lock().expect("test store lock") = Some(json.to_string());
    }

    fn load(&self) -> Option<String> {
        self.0.lock().expect("test store lock").clone()
    }

    fn clear(&self) {
        *self.0.lock().expect("test store lock") = None;
    }
}

#[cfg(test)]
impl SaveStore {
    /// An in-memory store plus the shared cell tests inspect and seed.
    pub(crate) fn in_memory() -> (Self, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
        (
            Self::with_backend(MemoryBackend(std::sync::Arc::clone(&cell))),
            cell,
        )
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveStore>()
            .add_message::<SaveRequested>()
            .add_systems(Update, persist_on_request)
            .add_systems(OnEnter(GameState::GameOver), delete_save);
    }
}

/// Turns pending [`SaveRequested`] messages into one stored snapshot of the
/// run. Skips (with a warning) if any run resource is missing — autosaves
/// only fire mid-run, where all of them exist.
// A Bevy system: each parameter is a distinct ECS handle for one of the six
// run-scoped resources being snapshotted.
#[allow(clippy::too_many_arguments)]
fn persist_on_request(
    mut requests: MessageReader<SaveRequested>,
    store: Res<SaveStore>,
    player: Option<Res<PlayerCharacter>>,
    level: Option<Res<Level>>,
    wallet: Option<Res<Wallet>>,
    owned: Option<Res<OwnedItems>>,
    equipment: Option<Res<PlayerEquipment>>,
    ladder: Option<Res<LadderProgress>>,
) {
    if requests.is_empty() {
        return;
    }
    requests.clear();
    let (Some(player), Some(level), Some(wallet), Some(owned), Some(equipment), Some(ladder)) =
        (player, level, wallet, owned, equipment, ladder)
    else {
        warn!("autosave requested without a full run in place; nothing saved");
        return;
    };
    let save = SaveGame::capture(&player, &level, &wallet, &owned, &equipment, &ladder);
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
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::combat::{CombatLogEvent, CombatSide};
    use crate::core::CorePlugin;
    use crate::progression::{
        FightOutcome, ProgressionPlugin, STARTING_GALBENI,
        result_ui::{AllocateAction, GameOverAction},
    };
    use crate::roster::RosterPlugin;
    use crate::shop::{ShopAction, ShopPlugin};

    /// A mid-run set of resources: a leveled character with gear, gold, and
    /// ladder progress into the second lap.
    fn sample_run() -> (
        PlayerCharacter,
        Level,
        Wallet,
        OwnedItems,
        PlayerEquipment,
        LadderProgress,
    ) {
        let player = PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes {
                putere: 5,
                agilitate: 3,
                vitalitate: 7,
                noroc: 2,
            },
            appearance: PlayerAppearance {
                skin_tone: crate::character::SkinTone::Olive,
                build: crate::character::BodyBuild::Sturdy,
                hair: crate::character::HairStyle::Tied,
                accent: crate::character::AccentColor::Gold,
            },
        };
        let level = Level {
            level: 4,
            xp: 120,
            unspent_points: 3,
        };
        let wallet = Wallet(365);
        let owned = OwnedItems(HashSet::from([
            ItemId::Palos,
            ItemId::BataCiobaneasca,
            ItemId::ScutDeLemn,
        ]));
        let mut equipment = Equipment::default();
        equipment.equip(ItemId::Palos);
        equipment.equip(ItemId::ScutDeLemn);
        let ladder = LadderProgress(12);
        (
            player,
            level,
            wallet,
            owned,
            PlayerEquipment(equipment),
            ladder,
        )
    }

    fn sample_save() -> SaveGame {
        let (player, level, wallet, owned, equipment, ladder) = sample_run();
        SaveGame::capture(&player, &level, &wallet, &owned, &equipment, &ladder)
    }

    #[test]
    fn capture_mirrors_every_resource_field() {
        let save = sample_save();
        assert_eq!(save.version, SAVE_VERSION);
        assert_eq!(save.name, "Făt-Frumos");
        assert_eq!(
            save.attrs,
            SavedAttributes {
                putere: 5,
                agilitate: 3,
                vitalitate: 7,
                noroc: 2,
            }
        );
        assert_eq!(
            save.appearance,
            PlayerAppearance {
                skin_tone: crate::character::SkinTone::Olive,
                build: crate::character::BodyBuild::Sturdy,
                hair: crate::character::HairStyle::Tied,
                accent: crate::character::AccentColor::Gold,
            }
        );
        assert_eq!(save.level, 4);
        assert_eq!(save.xp, 120);
        assert_eq!(save.unspent_points, 3);
        assert_eq!(save.wallet, 365);
        assert_eq!(
            save.owned_items,
            vec!["BataCiobaneasca", "Palos", "ScutDeLemn"],
            "owned items are sorted for deterministic JSON"
        );
        assert_eq!(
            save.equipped,
            vec!["Palos", "ScutDeLemn"],
            "equipped items in slot order"
        );
        assert_eq!(save.ladder_progress, 12);
        assert_eq!(save.lap, 2, "index 12 sits on the second lap");
    }

    #[test]
    fn json_roundtrip_preserves_every_field() {
        let save = sample_save();
        let json = save.to_json().expect("plain data serializes");
        let restored = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(restored, save);
    }

    #[test]
    fn resources_survive_the_full_reconstruction_exactly() {
        let (player, level, wallet, owned, equipment, ladder) = sample_run();
        let json = SaveGame::capture(&player, &level, &wallet, &owned, &equipment, &ladder)
            .to_json()
            .expect("plain data serializes");
        let save = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(save.player_character(), player);
        assert_eq!(save.level(), level);
        assert_eq!(save.wallet(), wallet);
        assert_eq!(save.owned_items(), owned);
        assert_eq!(save.player_equipment(), equipment);
        assert_eq!(save.ladder_progress(), ladder);
    }

    #[test]
    fn corrupt_json_is_rejected_without_panicking() {
        for corrupt in [
            "",
            "not json at all",
            "{",
            "42",
            r#"{"version":1}"#,
            r#"{"version":1,"name":"x","attrs":{"putere":-1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
        ] {
            assert!(
                SaveGame::from_json(corrupt).is_none(),
                "{corrupt:?} must be rejected"
            );
        }
    }

    #[test]
    fn a_version_mismatch_discards_the_save() {
        let mut save = sample_save();
        save.version = SAVE_VERSION + 1;
        let json = save.to_json().expect("plain data serializes");
        assert!(SaveGame::from_json(&json).is_none());
    }

    #[test]
    fn a_v1_save_without_appearance_defaults_cleanly() {
        let json = r#"{"version":1,"name":"Făt-Frumos","attrs":{"putere":5,"agilitate":3,"vitalitate":7,"noroc":2},"level":4,"xp":120,"unspent_points":3,"wallet":365,"owned_items":["BataCiobaneasca","Palos","ScutDeLemn"],"equipped":["Palos","ScutDeLemn"],"ladder_progress":12,"lap":2}"#;
        let save = SaveGame::from_json(json).expect("old v1 save still loads");
        assert_eq!(save.appearance, PlayerAppearance::default());
        assert_eq!(
            save.player_character().appearance,
            PlayerAppearance::default()
        );
    }

    #[test]
    fn an_unknown_item_name_discards_the_save() {
        let mut save = sample_save();
        save.owned_items.push("SabiaLuiStefan".to_string());
        let json = save.to_json().expect("plain data serializes");
        assert!(
            SaveGame::from_json(&json).is_none(),
            "unknown owned item invalidates the save"
        );

        let mut save = sample_save();
        save.equipped = vec!["NuExista".to_string()];
        let json = save.to_json().expect("plain data serializes");
        assert!(
            SaveGame::from_json(&json).is_none(),
            "unknown equipped item invalidates the save"
        );
    }

    #[test]
    fn load_save_clears_an_invalid_store() {
        let (store, cell) = SaveStore::in_memory();
        store.store("definitely not a save");
        assert!(load_save(&store).is_none());
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "the corrupt save is cleared, not re-read forever"
        );
    }

    #[test]
    fn load_save_returns_a_valid_snapshot_and_keeps_it_stored() {
        let (store, cell) = SaveStore::in_memory();
        let save = sample_save();
        store.store(&save.to_json().expect("plain data serializes"));
        assert_eq!(load_save(&store), Some(save));
        assert!(
            cell.lock().expect("test store lock").is_some(),
            "a valid save stays stored"
        );
    }

    // --- autosave and delete flows ---

    /// Headless app with the whole run flow (progression, roster, shop) and
    /// the save plugin over an in-memory store.
    fn test_app() -> (App, Arc<Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
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
