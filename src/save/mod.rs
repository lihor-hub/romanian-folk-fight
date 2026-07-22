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
//! Autosave points write a [`SaveRequested`] message, carrying the exact
//! [`ResumeDestination`] that checkpoint implies (hero confirmation and the
//! result/reward payout resume into the arena; shop entry and every shop
//! purchase/equip resume into the shop -- see each call site's own doc
//! comment for why); [`persist_on_request`] turns it into a stored snapshot
//! tagged with that destination. A run is one life: game over deletes the
//! save. The main menu's **Continuă** button restores the snapshot into the
//! run resources and emits exactly one [`crate::flow::FlowIntent`] --
//! `ContinueRun` (arena) or `ContinueToShop` (shop) -- chosen from the saved
//! [`SaveGame::resume_destination`] (#217); see `crate::menu`'s **Continuă**
//! handler. **Abandonează** (`crate::combat::pause`) is not an autosave
//! checkpoint at all: it forfeits the run outright, clearing the snapshot via
//! [`SaveStore::clear`] directly rather than persisting anything (#217).
//! Corrupt, partially-written, or unsupported-version saves never panic (see
//! [`snapshot::SaveGame::load`]); the menu offers a recovery action instead
//! of the game silently discarding them (#201).

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
use crate::roster::{CampaignSeed, LadderProgress, PrepareEncounterSet, PreparedEncounter};
use crate::shop::{OwnedItems, PlayerEquipment};

/// The `localStorage` key of the wasm backend. The `_v1` names the storage
/// bucket, not the schema version — the schema version lives inside the
/// payload's `"version"` field (see [`snapshot::CURRENT_VERSION`]), which is
/// what lets [`snapshot::SaveGame::from_json`] migrate in place without ever
/// needing a new storage location.
pub const STORAGE_KEY: &str = "rff_save_v1";

/// Fired by the autosave hooks (hero confirmation, victory payout, level-up
/// confirm, shop entry, shop purchase/equip) carrying the exact
/// [`ResumeDestination`] that checkpoint implies; [`persist_on_request`]
/// snapshots the run in response, tagged with that destination (#217).
#[derive(Message, Debug, Clone, Copy)]
pub struct SaveRequested(pub ResumeDestination);

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
                persist_on_request
                    .after(crate::flow::FlowIntentEmission)
                    .after(PrepareEncounterSet),
            )
            .add_systems(OnEnter(GameState::GameOver), delete_save);
    }
}

/// Turns pending [`SaveRequested`] messages into one stored snapshot of the
/// run, tagged with the requested [`ResumeDestination`]. Skips (with a
/// warning) if any run resource is missing — autosaves only fire mid-run,
/// where all of them exist. If more than one [`SaveRequested`] somehow queues
/// in the same frame, the last one's destination wins (matching
/// `crate::flow::apply_flow_intents`'s "effective, most-recent" handling of a
/// same-frame duplicate) — every current call site only ever writes at most
/// one per frame, so this is a defensive tie-break, not a real case.
// A Bevy system: each parameter is a distinct ECS handle for one of the
// run-scoped resources being snapshotted (see `snapshot`'s ownership
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
    campaign_seed: Option<Res<CampaignSeed>>,
    prepared_encounter: Option<Res<PreparedEncounter>>,
) {
    let Some(SaveRequested(resume_destination)) = requests.read().last().copied() else {
        return;
    };
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
        campaign_seed.as_deref().copied().unwrap_or_default(),
        prepared_encounter.as_deref(),
        resume_destination,
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
        let appearance = PlayerAppearance {
            skin_tone: crate::character::SkinTone::Deep,
            build: crate::character::BodyBuild::Balanced,
            hair: crate::character::HairStyle::Braided,
            accent: crate::character::AccentColor::Storm,
        };
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes {
                putere: 4,
                agilitate: 2,
                vitalitate: 4,
                noroc: 3,
                atac: 1,
                aparare: 2,
                carisma: 1,
                magie: 0,
            },
            appearance,
            definition: crate::character::CharacterDefinition::legacy_human(appearance),
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

    /// `pub(super)`: also used by the sibling `journeys` module below.
    pub(super) fn stored_save(cell: &Arc<Mutex<Option<String>>>) -> Option<SaveGame> {
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

    /// #217: arriving in the shop is itself a safe checkpoint (see
    /// `shop::autosave_on_shop_entry`) -- distinct from the purchase/equip
    /// autosave this test is actually about, so the entry autosave is
    /// consumed and cleared before pressing the rejected purchase button.
    #[test]
    fn entering_the_shop_autosaves_with_the_shop_resume_destination() {
        let (mut app, cell) = test_app();
        set_state(&mut app, GameState::Shop);

        let save = stored_save(&cell).expect("shop entry autosaves immediately");
        assert_eq!(
            save.resume_destination(),
            ResumeDestination::Shop,
            "the shop-entry checkpoint resumes back into the shop"
        );
    }

    #[test]
    fn a_failed_purchase_does_not_save_beyond_the_shop_entry_checkpoint() {
        let (mut app, cell) = test_app();
        set_state(&mut app, GameState::Shop);
        // Consume (and clear) the shop-entry autosave so this test isolates
        // the purchase-specific autosave path.
        cell.lock().expect("test store lock").take();

        let button = app
            .world_mut()
            .query_filtered::<(Entity, &ShopAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == ShopAction::Item(ItemId::Palos))
            .map(|(entity, _)| entity)
            .expect("item button exists");
        press(&mut app, button); // 150 > 50: rejected

        assert_eq!(
            stored_save(&cell),
            None,
            "nothing bought, nothing saved beyond the shop-entry checkpoint"
        );
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

/// #217 full-journey coverage: unlike `tests` above (which drives one
/// autosave/delete hook at a time), these tests cross screen boundaries --
/// menu -> creation -> fight -> abandon, and result -> shop -> a *simulated
/// browser reload* -> shop -- through the real production button handlers
/// (`crate::menu`, `crate::creation`, `crate::progression::result_ui`,
/// `crate::shop`, `crate::combat::pause`), never a raw `FlowIntent` write or
/// a hand-constructed `SaveGame`. Run with `cargo test save::journeys --lib`.
#[cfg(test)]
mod journeys {
    use std::sync::{Arc, Mutex};

    use bevy::state::app::StatesPlugin;

    use super::tests::stored_save;
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::combat::pause::PauseAction;
    use crate::combat::{CombatPlugin, CombatSide, PauseState};
    use crate::core::CorePlugin;
    use crate::creation::{
        CreationAction, CreationPlugin, HeroChoice, HeroPreset, PlayerCharacter,
    };
    use crate::flow::FlowPlugin;
    use crate::items::ItemId;
    use crate::menu::{DisabledButton, MenuAction, MenuPlugin};
    use crate::progression::result_ui::ResultAction;
    use crate::progression::{FightOutcome, Level, ProgressionPlugin};
    use crate::roster::{LadderProgress, RosterPlugin};
    use crate::shop::{OwnedItems, PlayerEquipment, ShopAction, ShopPlugin};
    use crate::town::{TownAction, TownPlugin};

    /// Every plugin a full menu -> creation -> fight (-> shop) journey
    /// touches, headless: `ArenaPlugin`/`CombatPlugin` (the fight screen and
    /// its pause overlay -- `combat::pause`'s own tests already prove this
    /// combination runs without an `AssetServer`), `RosterPlugin`/
    /// `ProgressionPlugin`/`ShopPlugin` (the run-scoped resources
    /// `SaveGame::capture`/`restore` cover), `MenuPlugin`/`CreationPlugin`
    /// (the two screens these journeys start from), and `SavePlugin` over an
    /// in-memory store (never this machine's real save location).
    /// `seed_json`, if given, is stored in the in-memory [`SaveStore`]
    /// *before* the app ever boots to `MainMenu` -- required for the
    /// "reload" half of the shop journey below, where the menu's
    /// `Continuă`/disabled state (locked in the moment `OnEnter(MainMenu)`
    /// spawns it) must already reflect the pre-seeded snapshot, the same
    /// reasoning `menu::tests::test_app_with_save`'s own doc comment gives.
    fn journey_test_app_with_seed(seed_json: Option<&str>) -> (App, Arc<Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            MenuPlugin,
            CreationPlugin,
            ArenaPlugin,
            CombatPlugin,
            RosterPlugin,
            ProgressionPlugin,
            TownPlugin,
            ShopPlugin,
            SavePlugin,
        ));
        let (store, cell) = SaveStore::in_memory();
        if let Some(json) = seed_json {
            store.store(json);
        }
        app.insert_resource(store);
        // `combat::pause::toggle_on_esc` (part of `CombatPlugin`) reads
        // `Res<ButtonInput<KeyCode>>` unconditionally -- `MinimalPlugins`
        // does not provide it (that's `InputPlugin`'s job), so this journey
        // needs it explicitly, exactly like `combat::pause`'s own tests do.
        app.init_resource::<ButtonInput<KeyCode>>();
        app.update(); // headless `Loading` fall-through queues MainMenu (#114)
        app.update(); // transition applies; `OnEnter(MainMenu)` spawns the menu
        (app, cell)
    }

    fn journey_test_app() -> (App, Arc<Mutex<Option<String>>>) {
        journey_test_app_with_seed(None)
    }

    fn current_state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    /// Finds the one button entity carrying `action` -- works for any of the
    /// screens' `Component + Copy + PartialEq` action enums
    /// (`MenuAction`/`CreationAction`/`ResultAction`/`ShopAction`/
    /// `PauseAction`), so this journey module needs exactly one finder
    /// instead of one per screen.
    fn find_button<A: Component + Copy + PartialEq>(app: &mut App, action: A) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &A), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .unwrap_or_else(|| panic!("no button carries the requested action"))
    }

    /// Presses `entity` and settles any in-screen (non-navigating) reaction:
    /// one `app.update()` is enough for a handler that only mutates a draft/
    /// resource, never a `GameState` transition.
    fn press_in_screen(app: &mut App, entity: Entity) {
        app.world_mut()
            .entity_mut(entity)
            .insert(Interaction::Pressed);
        app.update();
    }

    /// Presses `entity` and settles the `FlowIntent` it emits all the way
    /// through to the resulting `GameState`: the first `app.update()` runs
    /// the handler (domain side effect + intent write) and queues the
    /// transition; the second applies it -- the same two-update contract
    /// `crate::flow`'s own tests document.
    fn press_and_transition(app: &mut App, entity: Entity) {
        app.world_mut()
            .entity_mut(entity)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    /// Drives menu -> creation -> a confirmed hero -> `GameState::Town` ->
    /// `GameState::Fight`, exactly as a player would (through the real
    /// `NewGame`/`SelectChoice`/`Confirm`/`EnterArena` button handlers) --
    /// the shared first half of both journeys below (#129: the run starts at
    /// the hub and the arena is entered from there).
    fn start_new_run_into_fight(app: &mut App, preset: HeroPreset) {
        let new_game = find_button(app, MenuAction::NewGame);
        press_and_transition(app, new_game);
        assert_eq!(current_state(app), GameState::CharacterCreation);

        let select_preset = find_button(
            app,
            CreationAction::SelectChoice(HeroChoice::Preset(preset)),
        );
        press_in_screen(app, select_preset);
        let confirm = find_button(app, CreationAction::Confirm);
        press_and_transition(app, confirm);
        assert_eq!(current_state(app), GameState::Town);

        let enter_arena = find_button(app, TownAction::EnterArena);
        press_and_transition(app, enter_arena);
        assert_eq!(current_state(app), GameState::Fight);
    }

    // --- Journey 1: new run -> Fight -> Abandon -> Main Menu, Continuă disabled ---

    /// The full abandon-forfeit journey (#217's acceptance criteria): a
    /// brand-new run reaches the fight (autosaving the hero-confirmation
    /// checkpoint), the player abandons via the real pause-overlay button,
    /// and the result is the main menu with the run snapshot gone, every
    /// run-scoped resource reset, and no `MenuAction::Continue` left for any
    /// button to carry -- never a fresh full-health retry of the abandoned
    /// fight.
    #[test]
    fn new_run_to_fight_then_abandon_returns_to_menu_with_continue_disabled() {
        let (mut app, cell) = journey_test_app();

        start_new_run_into_fight(&mut app, HeroPreset::Voinicul);

        let hero_confirm_save = stored_save(&cell)
            .expect("hero confirmation autosaves a run -- something exists to forfeit");
        assert_eq!(
            hero_confirm_save.resume_destination(),
            ResumeDestination::Town,
            "the hero-confirmation checkpoint resumes into the town hub (#129)"
        );

        // Open the pause overlay -- a `PauseState` substate change with no
        // domain side effect of its own (see `combat::pause::toggle_on_esc`,
        // which a real Esc keypress drives; this journey only needs the
        // resulting substate, not the keyboard path itself).
        app.world_mut()
            .resource_mut::<NextState<PauseState>>()
            .set(PauseState::Paused);
        app.update();

        let abandon = find_button(&mut app, PauseAction::Abandon);
        press_and_transition(&mut app, abandon);

        assert_eq!(
            current_state(&app),
            GameState::MainMenu,
            "abandon returns to the main menu"
        );
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "abandon forfeits the run: the snapshot is cleared"
        );
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_none(),
            "no confirmed hero survives a forfeit -- no fresh full-health retry is possible"
        );
        assert_eq!(*app.world().resource::<Wallet>(), Wallet::default());
        assert_eq!(*app.world().resource::<Level>(), Level::default());
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress::default(),
            "run state is reset exactly like a game-over or a fresh new game"
        );

        // The re-spawned main menu must offer no way to resume: no button
        // anywhere carries `MenuAction::Continue` (the `SnapshotLoad::NoSave`
        // arm of `menu::spawn_main_menu` spawns only the disabled marker),
        // and the disabled marker itself is present.
        let continue_exists = app
            .world_mut()
            .query::<&MenuAction>()
            .iter(app.world())
            .any(|action| *action == MenuAction::Continue);
        assert!(
            !continue_exists,
            "no MenuAction::Continue must exist after a forfeit -- Continuă can't resume anything"
        );
        let disabled_marker_count = app
            .world_mut()
            .query_filtered::<(), With<DisabledButton>>()
            .iter(app.world())
            .count();
        assert_eq!(
            disabled_marker_count, 1,
            "the greyed-out Continuă marker is shown instead"
        );
    }

    // --- Journey 2: result -> Shop -> a simulated reload -> Shop ---

    /// Drives menu -> creation -> a won first fight -> the result screen ->
    /// the shop, buying one affordable item -- the exact journey
    /// `xtask::web_smoke::save_reload` drives in a real browser. Returns the
    /// stored JSON (as [`SaveStore::load`] would hand back to a fresh page
    /// load) plus the exact resource values it should restore, for the
    /// "reload" half of the journey to compare against.
    fn play_result_to_shop_with_a_purchase(
        app: &mut App,
        cell: &Arc<Mutex<Option<String>>>,
    ) -> (String, Wallet, LadderProgress, OwnedItems, PlayerEquipment) {
        start_new_run_into_fight(app, HeroPreset::Voinicul);

        // Simulate winning the first fight exactly like
        // `save::tests::the_victory_payout_autosaves_the_credited_run` does
        // -- a `FightOutcome` inserted directly (the pure combat resolution
        // is `combat::engine`'s own concern, out of scope here), then the
        // real state transition credits the reward and autosaves.
        let opponent = app.world().resource::<LadderProgress>().opponent();
        app.insert_resource(FightOutcome::from_defeat(
            CombatSide::Player,
            opponent.level,
            opponent.is_boss,
        ));
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        assert_eq!(current_state(app), GameState::FightResult);

        let result_save = stored_save(cell).expect("the result/reward checkpoint autosaves");
        assert_eq!(
            result_save.resume_destination(),
            ResumeDestination::Town,
            "the result/reward checkpoint resumes into the town hub (#129)"
        );

        // #129: the shop is reached through the hub -- result Continuă ->
        // Town -> Prăvălie -> Shop, all via the real production buttons.
        let continue_button = find_button(app, ResultAction::Continue);
        press_and_transition(app, continue_button);
        assert_eq!(current_state(app), GameState::Town);
        let go_to_shop = find_button(app, TownAction::GoToShop);
        press_and_transition(app, go_to_shop);
        assert_eq!(current_state(app), GameState::Shop);

        let shop_entry_save = stored_save(cell).expect("shop entry autosaves immediately");
        assert_eq!(
            shop_entry_save.resume_destination(),
            ResumeDestination::Shop,
            "arriving in the shop resumes back into the shop"
        );

        // A shop change (#217's "shop changes" checkpoint): buy one
        // affordable item Voinicul does not start with.
        let buy_caciula = find_button(app, ShopAction::Item(ItemId::CaciulaDeOaie));
        press_in_screen(app, buy_caciula);

        let wallet = *app.world().resource::<Wallet>();
        let ladder = *app.world().resource::<LadderProgress>();
        let owned = app.world().resource::<OwnedItems>().clone();
        let equipment = app.world().resource::<PlayerEquipment>().clone();
        assert!(
            owned.0.contains(&ItemId::CaciulaDeOaie),
            "the purchase is reflected in OwnedItems"
        );

        let json = cell
            .lock()
            .expect("test store lock")
            .clone()
            .expect("the purchase autosaves, tagged with the shop destination");
        let save = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(
            save.resume_destination(),
            ResumeDestination::Shop,
            "a shop purchase keeps resuming into the shop"
        );

        (json, wallet, ladder, owned, equipment)
    }

    /// #217's headline acceptance criterion: "result → shop → reload →
    /// shop". "Reload" is simulated the way it actually happens for a
    /// player: the exact JSON one app captured is the only thing that
    /// crosses into a **brand-new** headless `App` (never the same
    /// resources/entities -- a real page reload re-boots the wasm module
    /// from scratch), which then restores through `MenuAction::Continue`
    /// exactly like the real menu handler does.
    #[test]
    fn result_to_shop_then_reload_resumes_at_shop_with_every_run_value_intact() {
        let (mut before_app, before_cell) = journey_test_app();
        let (json, wallet, ladder, owned, equipment) =
            play_result_to_shop_with_a_purchase(&mut before_app, &before_cell);

        // A brand-new app -- the "reloaded page" -- with its own fresh
        // in-memory store, pre-seeded with exactly the bytes the first app's
        // store held *before* it ever boots to MainMenu (so Continuă's
        // enabled state, locked in at spawn, already reflects the seeded
        // snapshot). Nothing else survives from `before_app`.
        let (mut after_app, after_cell) = journey_test_app_with_seed(Some(&json));

        let continue_button = find_button(&mut after_app, MenuAction::Continue);
        press_and_transition(&mut after_app, continue_button);

        assert_eq!(
            current_state(&after_app),
            GameState::Shop,
            "Continuă resumes straight into the shop, matching the saved destination"
        );
        assert_eq!(*after_app.world().resource::<Wallet>(), wallet);
        assert_eq!(*after_app.world().resource::<LadderProgress>(), ladder);
        assert_eq!(*after_app.world().resource::<OwnedItems>(), owned);
        assert_eq!(*after_app.world().resource::<PlayerEquipment>(), equipment);
        assert_eq!(
            stored_save(&after_cell).as_ref().map(SaveGame::to_json),
            stored_save(&before_cell).as_ref().map(SaveGame::to_json),
            "the restored run resources round-trip back to the exact same snapshot"
        );
    }
}
