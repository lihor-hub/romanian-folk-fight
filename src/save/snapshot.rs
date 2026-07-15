//! Run snapshot schema and versioned migration (#193), kept separate from
//! *where* the JSON physically lives (`src/save/mod.rs`'s [`super::SaveStore`]
//! / [`super::SaveBackend`]). [`SaveGame`] is the always-current schema; each
//! superseded version keeps a private `SnapshotV{n}` struct here purely so
//! [`SaveGame::from_json`] can parse and migrate old saves forward — nothing
//! else in the codebase ever constructs one.
//!
//! # Version envelope
//!
//! Every stored save is a JSON object carrying a `"version"` field.
//! [`SaveGame::from_json`] peeks at just that field ([`VersionProbe`]) before
//! deciding which versioned struct to deserialize the rest of the payload
//! as, then folds it forward one [`Migrate`] step at a time until it reaches
//! [`CURRENT_VERSION`]. A version older than any known struct, newer than
//! [`CURRENT_VERSION`], JSON that fails to parse as its claimed version's
//! shape, or a reference to an item outside [`crate::items::ItemId::ALL`] —
//! all fail closed (`None` plus a `warn!`, never a panic). See this module's
//! `a_future_version_is_rejected_without_panic`,
//! `corrupt_json_is_rejected_without_panicking`, and
//! `an_unknown_item_name_discards_the_save` tests.
//!
//! # v1 → v2 safe-default table (#193)
//!
//! | v2 field | v1 source | migrated value | why it's safe |
//! |---|---|---|---|
//! | `lifetime_earnings` | *(new)* | `v1.wallet` | v1 never tracked lifetime earnings separately from the spendable wallet; the current balance is the highest value known not to fabricate galbeni the run never visibly had |
//! | `resume_destination` | *(new)* | [`ResumeDestination::Fight`] | the only destination v1's **Continuă** button ever produced (see the pre-#193 doc on `SaveGame::restore`) |
//! | everything else | `v1.*` | carried over verbatim | no new information needed |
//!
//! # Extending to a new version (recipe for #133/#137/#140)
//!
//! Each of those issues owns exactly one new version and follows the same
//! five steps #193 used for v1 → v2:
//!
//! 1. Add a new struct (e.g. `SnapshotV3`) with every field the new version
//!    owns — the old `SaveGame` fields plus whatever new run-scoped values
//!    that issue introduces (#133: persistent current HP/consumables/
//!    effects; #137: tutorial completion; #140: current matchup/bracket/HP/
//!    stamina). Give any field that might arrive from a same-version
//!    additive patch later `#[serde(default)]`, matching `appearance`,
//!    `lifetime_earnings`, and `resume_destination` above.
//! 2. Rename the current `SaveGame` struct to `SnapshotV2` (kept private,
//!    migration-only) and define a new `SaveGame` with the new version's
//!    shape — the public name never changes, so no caller needs to know a
//!    version bump happened.
//! 3. Implement `impl Migrate for SnapshotV2 { type Next = SaveGame; fn
//!    migrate(self) -> SaveGame { ... } }`, giving every new field an
//!    explicit, documented default (not a bare `Default::default()` unless
//!    that genuinely is the safe value) and a row in a new default table
//!    like the one above.
//! 4. Bump `CURRENT_VERSION`, and extend the match in
//!    [`SaveGame::from_json`]: the arm that used to read
//!    `CURRENT_VERSION => serde_json::from_str::<Self>(json)` becomes `2 =>
//!    serde_json::from_str::<SnapshotV2>(json).ok()?.migrate()`, chained the
//!    same way `1 =>` already folds v1 forward; the new `CURRENT_VERSION`
//!    arm parses the new shape directly.
//! 5. Extend [`SaveGame::capture`]/[`SaveGame::restore`]/[`reset`] for the
//!    newly-owned resources, and add fixture/migration/round-trip/reset
//!    tests shaped like this module's v1 ones, plus the fail-closed trio
//!    (corrupt input, unknown item, future version).
//!
//! No screen code (`menu`, `creation`, `shop`, `flow`, ...) needs to change:
//! they only ever call [`SaveGame::capture`]/[`SaveGame::from_json`]/
//! [`SaveGame::restore`]/[`reset`] generically, never version-specific code
//! (see `handle_menu_actions` in `crate::menu`, the sole production caller of
//! `restore`).
//!
//! # Run-field ownership contract
//!
//! [`SaveGame`]'s fields are the single authoritative list of run-scoped
//! values. [`reset`] derives its defaults from exactly that list — the same
//! fields `capture`/`restore` touch — so a fresh run can never drift from
//! what a mid-run save can hold. Before #193, `progression::reset_run` hand-
//! maintained a second list that happened to agree with the save schema by
//! coincidence rather than construction; this module closes that gap.
//! `PlayerCharacter` is the one field with no "reset" value of its own: a
//! fresh run has no confirmed hero yet (character creation hasn't run), so
//! [`reset`] removes it rather than defaulting it, exactly like the pre-#193
//! behavior.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::{Attributes, PlayerAppearance};
use crate::creation::PlayerCharacter;
use crate::items::{Equipment, ItemId, Slot};
use crate::progression::{Level, LifetimeEarnings, Wallet};
use crate::roster::LadderProgress;
use crate::shop::{OwnedItems, PlayerEquipment};

/// The version written into every save produced by this build; loads of any
/// other value either migrate forward (if older and known, see [`Migrate`])
/// or are discarded (if unknown/newer).
pub const CURRENT_VERSION: u32 = 2;

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

/// A screen safe to resume a run into via **Continuă** — deliberately a
/// small, closed set rather than the raw [`crate::core::GameState`] (which
/// has states, like `FightResult` or `CharacterCreation`, that are never
/// safe to land on directly from a stored snapshot). Pure save-schema data:
/// nothing reads it back out of a *restored* run (no gameplay system tracks
/// "the current resume destination" as a live resource), so
/// [`SaveGame::restore`] does not insert it as one -- it is only ever read
/// off the still-serialized [`SaveGame`] by `crate::menu`'s **Continuă**
/// handler (#217), which turns it into exactly one
/// [`crate::flow::FlowIntent::ContinueRun`]/[`crate::flow::FlowIntent::ContinueToShop`]
/// (never interpreted as a raw field by any other screen -- see
/// `crate::flow`'s ownership-boundary docs).
///
/// #217 wires every safe checkpoint (hero confirmation, the result/reward
/// autosave, shop entry/purchases/equips, and the victory/lap autosave) to
/// pass the destination that actually matches where **Continuă** should
/// land, via [`SaveGame::capture`]'s explicit parameter -- there is no
/// implicit/default destination for a fresh capture, precisely so a new
/// checkpoint can never forget to pick one. A future safe destination (a
/// child of #133/#137/#140) adds a new variant here plus a new arm wherever
/// `crate::menu` maps [`ResumeDestination`] to a `FlowIntent` -- see that
/// module's doc comment for the exact extension steps.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResumeDestination {
    /// Resume straight into the next fight -- the destination for the hero-
    /// confirmation and result/reward checkpoints.
    #[default]
    Fight,
    /// Resume into the shop -- the destination for shop entry (whether
    /// reached from the result screen or from victory's next-lap), and every
    /// shop purchase/equip autosave (#217).
    Shop,
}

/// Just enough of a stored payload to read which version it claims to be,
/// before committing to a versioned struct to parse the rest as.
#[derive(Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Why [`SaveGame::load`] could not produce a [`SaveGame`] -- the reason
/// [`super::storage`] (#201) needs to pick a recoverable-vs-not menu
/// treatment, kept separate from a bare `None` so that distinction survives
/// past this module's parse/migrate/validate pipeline. Added for #201;
/// deliberately does not change the pipeline itself (still fails closed on
/// exactly the same inputs `from_json` always has -- see this module's
/// `corrupt_json_is_rejected_without_panicking` and
/// `a_future_version_is_rejected_without_panic` tests, both still green).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotLoadError {
    /// The payload is unusable on its own terms: it fails to parse as JSON
    /// at all, fails to parse as the shape its claimed version implies (e.g.
    /// truncated by a torn write), claims a version this build has never
    /// known how to read (older than any known [`Migrate`] source and not
    /// [`CURRENT_VERSION`]), or references an item name outside
    /// [`crate::items::ItemId::ALL`]. Never becomes readable by waiting --
    /// the stored bytes themselves are the problem.
    Invalid,
    /// The payload parses its version field as strictly newer than
    /// [`CURRENT_VERSION`] -- written by a newer build than this one (e.g.
    /// after a rollback). Reported separately from [`Self::Invalid`] because
    /// the payload is not necessarily corrupt, just from the future.
    FutureVersion,
}

/// v1 payload (pre-#193, `SAVE_VERSION == 1`): kept only so
/// [`SaveGame::from_json`] can parse and [`Migrate`] old saves. Nothing else
/// in the codebase constructs this — new saves are always [`SaveGame`].
#[derive(Deserialize, Debug, Clone)]
struct SnapshotV1 {
    name: String,
    attrs: SavedAttributes,
    #[serde(default)]
    appearance: PlayerAppearance,
    level: u32,
    xp: u32,
    unspent_points: u32,
    wallet: u32,
    owned_items: Vec<String>,
    equipped: Vec<String>,
    ladder_progress: usize,
    lap: u32,
}

/// Migrates one schema version's payload into the next. Each future version
/// (#133/#137/#140 — see this module's extension recipe) adds one impl of
/// this, chained by [`SaveGame::from_json`].
trait Migrate {
    type Next;
    fn migrate(self) -> Self::Next;
}

impl Migrate for SnapshotV1 {
    type Next = SaveGame;

    /// See the v1 → v2 default table in this module's docs for the
    /// rationale behind `lifetime_earnings` and `resume_destination`.
    fn migrate(self) -> SaveGame {
        SaveGame {
            version: CURRENT_VERSION,
            name: self.name,
            attrs: self.attrs,
            appearance: self.appearance,
            level: self.level,
            xp: self.xp,
            unspent_points: self.unspent_points,
            wallet: self.wallet,
            lifetime_earnings: self.wallet,
            owned_items: self.owned_items,
            equipped: self.equipped,
            ladder_progress: self.ladder_progress,
            lap: self.lap,
            resume_destination: ResumeDestination::Fight,
        }
    }
}

/// One full run snapshot (v2, #193): mirrors every run-scoped resource — the
/// confirmed character, the experience state, the wallet and lifetime
/// earnings, the shop purchases and loadout, the ladder position, and the
/// typed safe resume destination.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SaveGame {
    /// Always [`CURRENT_VERSION`] for freshly captured saves; older, known
    /// versions migrate forward on load (see [`SaveGame::from_json`]) and
    /// unknown/newer ones are discarded.
    pub version: u32,
    /// [`PlayerCharacter::name`].
    pub name: String,
    /// [`PlayerCharacter::attributes`].
    pub attrs: SavedAttributes,
    /// [`PlayerCharacter::appearance`]. Missing on pre-appearance v1 saves,
    /// where it defaults to the current project baseline.
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
    /// [`LifetimeEarnings`] total in galbeni. New in v2 (#193); missing on
    /// migrated v1 saves defaults to `wallet` (see the module docs' default
    /// table).
    #[serde(default)]
    pub lifetime_earnings: u32,
    /// [`OwnedItems`] as sorted item names, for deterministic JSON.
    pub owned_items: Vec<String>,
    /// [`PlayerEquipment`] as item names in [`Slot::ALL`] order.
    pub equipped: Vec<String>,
    /// [`LadderProgress`]: the 0-based index of the next opponent.
    pub ladder_progress: usize,
    /// The 1-based ladder lap; derived from `ladder_progress`, stored for
    /// human-readable saves.
    pub lap: u32,
    /// New in v2 (#193): see [`ResumeDestination`].
    #[serde(default)]
    pub resume_destination: ResumeDestination,
}

impl SaveGame {
    /// Snapshots the current run from its resources, tagged with the exact
    /// [`ResumeDestination`] **Continuă** should land on if this snapshot is
    /// the one later restored. #217: every call site is one of the safe
    /// checkpoints (hero confirmation, result/reward, shop entry/purchase/
    /// equip, victory/lap) and passes the destination that specific
    /// checkpoint implies -- there is deliberately no default here (unlike
    /// pre-#217, which always captured [`ResumeDestination::Fight`]), so a
    /// new checkpoint can never forget to choose one.
    // A Bevy-adjacent capture helper: each parameter is a distinct run-scoped
    // resource being snapshotted (see the module docs' ownership contract).
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        player: &PlayerCharacter,
        level: &Level,
        wallet: &Wallet,
        lifetime_earnings: &LifetimeEarnings,
        owned: &OwnedItems,
        equipment: &PlayerEquipment,
        ladder: &LadderProgress,
        resume_destination: ResumeDestination,
    ) -> Self {
        let mut owned_items: Vec<String> = owned.0.iter().map(|id| item_name(*id)).collect();
        owned_items.sort();
        let equipped = Slot::ALL
            .into_iter()
            .filter_map(|slot| equipment.0.equipped(slot))
            .map(item_name)
            .collect();
        Self {
            version: CURRENT_VERSION,
            name: player.name.clone(),
            attrs: player.attributes.into(),
            appearance: player.appearance,
            level: level.level,
            xp: level.xp,
            unspent_points: level.unspent_points,
            wallet: wallet.0,
            lifetime_earnings: lifetime_earnings.0,
            owned_items,
            equipped,
            ladder_progress: ladder.0,
            lap: ladder.lap(),
            resume_destination,
        }
    }

    /// The snapshot as JSON; `None` only if serialization itself fails
    /// (which plain data like this never does — handled instead of unwrapped
    /// to keep runtime code panic-free).
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    /// Parses, migrates, and validates a snapshot: corrupt JSON, an unknown
    /// or unsupported version, or an item name missing from the catalog all
    /// yield `None` — never a panic. See the module docs' version envelope
    /// section for the full fail-closed contract. A thin wrapper over
    /// [`Self::load`] for callers that only need "did it work", not *why* it
    /// didn't.
    pub fn from_json(json: &str) -> Option<Self> {
        Self::load(json).ok()
    }

    /// Like [`Self::from_json`], but keeps *why* a load failed instead of
    /// collapsing every failure into `None` — see [`SnapshotLoadError`].
    /// Added for #201, whose storage layer ([`super::storage`]) needs this
    /// distinction to decide whether the menu can offer a recovery action
    /// (it always can, today: both variants are equally unresumable) without
    /// re-implementing this module's own parse/migrate/validate pipeline.
    pub fn load(json: &str) -> Result<Self, SnapshotLoadError> {
        let probe: VersionProbe =
            serde_json::from_str(json).map_err(|_| SnapshotLoadError::Invalid)?;
        let save = match probe.version {
            1 => serde_json::from_str::<SnapshotV1>(json)
                .map_err(|_| SnapshotLoadError::Invalid)?
                .migrate(),
            CURRENT_VERSION => {
                serde_json::from_str::<Self>(json).map_err(|_| SnapshotLoadError::Invalid)?
            }
            other if other > CURRENT_VERSION => {
                warn!("save version {other} is newer than this build supports ({CURRENT_VERSION})");
                return Err(SnapshotLoadError::FutureVersion);
            }
            other => {
                warn!(
                    "save version {other} is not supported (current {CURRENT_VERSION}); discarding"
                );
                return Err(SnapshotLoadError::Invalid);
            }
        };
        if let Some(unknown) = save
            .owned_items
            .iter()
            .chain(&save.equipped)
            .find(|name| parse_item(name).is_none())
        {
            warn!("save references unknown item {unknown:?}; discarding");
            return Err(SnapshotLoadError::Invalid);
        }
        Ok(save)
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

    /// The saved [`LifetimeEarnings`].
    pub fn lifetime_earnings(&self) -> LifetimeEarnings {
        LifetimeEarnings(self.lifetime_earnings)
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

    /// The saved [`ResumeDestination`]. Not restored as an ECS resource (see
    /// its own docs) — read directly off the snapshot by whatever consumes
    /// it (#217).
    pub fn resume_destination(&self) -> ResumeDestination {
        self.resume_destination
    }

    /// Restores every run resource from the snapshot; with the resources in
    /// place, entering the resumed screen (today always
    /// [`crate::core::GameState::Fight`]) continues the run exactly.
    pub fn restore(&self, commands: &mut Commands) {
        commands.insert_resource(self.player_character());
        commands.insert_resource(self.level());
        commands.insert_resource(self.wallet());
        commands.insert_resource(self.lifetime_earnings());
        commands.insert_resource(self.owned_items());
        commands.insert_resource(self.player_equipment());
        commands.insert_resource(self.ladder_progress());
    }
}

/// Resets every run-scoped resource this snapshot owns to the value a fresh
/// run starts with — the single authoritative reset, derived from exactly
/// the same field list [`SaveGame::capture`]/[`SaveGame::restore`] use (see
/// this module's ownership-contract docs). `PlayerCharacter` has no reset
/// value of its own (a fresh run has no confirmed hero yet) so it is removed
/// rather than defaulted.
pub fn reset(commands: &mut Commands) {
    commands.remove_resource::<PlayerCharacter>();
    commands.insert_resource(Level::default());
    commands.insert_resource(Wallet::default());
    commands.insert_resource(LifetimeEarnings::default());
    commands.insert_resource(OwnedItems::default());
    commands.insert_resource(PlayerEquipment::default());
    commands.insert_resource(LadderProgress::default());
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::character::{AccentColor, BodyBuild, HairStyle, SkinTone};

    /// A mid-run set of resources: a leveled character with gear, gold, and
    /// ladder progress into the second lap.
    pub(crate) fn sample_run() -> (
        PlayerCharacter,
        Level,
        Wallet,
        LifetimeEarnings,
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
                skin_tone: SkinTone::Olive,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Tied,
                accent: AccentColor::Gold,
            },
        };
        let level = Level {
            level: 4,
            xp: 120,
            unspent_points: 3,
        };
        let wallet = Wallet(365);
        let lifetime_earnings = LifetimeEarnings(510);
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
            lifetime_earnings,
            owned,
            PlayerEquipment(equipment),
            ladder,
        )
    }

    /// A sample save whose resume destination is [`ResumeDestination::Fight`]
    /// -- the destination every existing fixture/migration/round-trip test in
    /// this module (and every consumer test in `save::mod`/`save::storage`)
    /// was already written against pre-#217, so it stays the default here
    /// rather than every one of those call sites needing to spell it out.
    pub(crate) fn sample_save() -> SaveGame {
        sample_save_with_destination(ResumeDestination::Fight)
    }

    /// Like [`sample_save`], but with an explicit [`ResumeDestination`] --
    /// used by this module's own destination-specific tests (e.g. the shop
    /// checkpoint).
    pub(crate) fn sample_save_with_destination(resume_destination: ResumeDestination) -> SaveGame {
        let (player, level, wallet, lifetime_earnings, owned, equipment, ladder) = sample_run();
        SaveGame::capture(
            &player,
            &level,
            &wallet,
            &lifetime_earnings,
            &owned,
            &equipment,
            &ladder,
            resume_destination,
        )
    }

    #[test]
    fn capture_mirrors_every_resource_field() {
        let save = sample_save();
        assert_eq!(save.version, CURRENT_VERSION);
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
                skin_tone: SkinTone::Olive,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Tied,
                accent: AccentColor::Gold,
            }
        );
        assert_eq!(save.level, 4);
        assert_eq!(save.xp, 120);
        assert_eq!(save.unspent_points, 3);
        assert_eq!(save.wallet, 365);
        assert_eq!(save.lifetime_earnings, 510);
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
        assert_eq!(
            save.resume_destination,
            ResumeDestination::Fight,
            "sample_save's destination defaults to Fight"
        );
    }

    /// #217: `capture` stores whichever [`ResumeDestination`] the caller
    /// passes -- e.g. the shop checkpoint's -- not a hardcoded default.
    #[test]
    fn capture_stores_the_shop_resume_destination_when_given_it() {
        let save = sample_save_with_destination(ResumeDestination::Shop);
        assert_eq!(save.resume_destination, ResumeDestination::Shop);
        let json = save.to_json().expect("plain data serializes");
        let restored = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(
            restored.resume_destination(),
            ResumeDestination::Shop,
            "the shop destination round-trips through JSON"
        );
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
        let (player, level, wallet, lifetime_earnings, owned, equipment, ladder) = sample_run();
        let json = SaveGame::capture(
            &player,
            &level,
            &wallet,
            &lifetime_earnings,
            &owned,
            &equipment,
            &ladder,
            ResumeDestination::Fight,
        )
        .to_json()
        .expect("plain data serializes");
        let save = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(save.player_character(), player);
        assert_eq!(save.level(), level);
        assert_eq!(save.wallet(), wallet);
        assert_eq!(save.lifetime_earnings(), lifetime_earnings);
        assert_eq!(save.owned_items(), owned);
        assert_eq!(save.player_equipment(), equipment);
        assert_eq!(save.ladder_progress(), ladder);
        assert_eq!(save.resume_destination(), ResumeDestination::Fight);
    }

    #[test]
    fn corrupt_json_is_rejected_without_panicking() {
        for corrupt in [
            "",
            "not json at all",
            "{",
            "42",
            r#"{"version":2}"#,
            r#"{"version":1}"#,
            // Negative putere: fails to parse as the u32 the v2 shape expects.
            r#"{"version":2,"name":"x","attrs":{"putere":-1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
            // Same corruption, but claiming to be a v1 payload — the v1
            // parse-then-migrate path must fail closed too, not just the
            // current-version path.
            r#"{"version":1,"name":"x","attrs":{"putere":-1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
        ] {
            assert!(
                SaveGame::from_json(corrupt).is_none(),
                "{corrupt:?} must be rejected"
            );
        }
    }

    #[test]
    fn a_future_version_is_rejected_without_panic() {
        let mut save = sample_save();
        save.version = CURRENT_VERSION + 1;
        let json = save.to_json().expect("plain data serializes");
        assert!(
            SaveGame::from_json(&json).is_none(),
            "a version newer than this build knows about must fail closed"
        );

        // An unknown *old* version (neither a known past version nor the
        // current one) fails closed the same way.
        assert!(SaveGame::from_json(r#"{"version":0}"#).is_none());
    }

    /// #201: [`SaveGame::load`] classifies exactly why a load failed, so
    /// [`super::super::storage`] can tell a corrupt/unsupported-old payload
    /// apart from one written by a newer build — both still fail closed
    /// (never a panic, never a resumed run), but only the typed distinction
    /// lets a caller describe *which* happened.
    #[test]
    fn load_classifies_a_future_version_separately_from_invalid_data() {
        let mut save = sample_save();
        save.version = CURRENT_VERSION + 1;
        let json = save.to_json().expect("plain data serializes");
        assert_eq!(SaveGame::load(&json), Err(SnapshotLoadError::FutureVersion));
    }

    #[test]
    fn load_classifies_corrupt_and_unsupported_old_data_as_invalid() {
        for corrupt in [
            "",
            "not json at all",
            "{",
            "42",
            r#"{"version":2}"#,
            r#"{"version":1}"#,
            r#"{"version":0}"#,
        ] {
            assert_eq!(
                SaveGame::load(corrupt),
                Err(SnapshotLoadError::Invalid),
                "{corrupt:?} must classify as Invalid, not FutureVersion"
            );
        }
    }

    #[test]
    fn load_classifies_an_unknown_item_as_invalid() {
        let mut save = sample_save();
        save.owned_items.push("SabiaLuiStefan".to_string());
        let json = save.to_json().expect("plain data serializes");
        assert_eq!(SaveGame::load(&json), Err(SnapshotLoadError::Invalid));
    }

    /// The exact v1 fixture this module's docs table describes, captured
    /// once from a real pre-#193 `SaveGame` so the migration is verified
    /// against a real save shape, not a hand-rolled approximation.
    fn exact_v1_fixture() -> &'static str {
        r#"{"version":1,"name":"Făt-Frumos","attrs":{"putere":5,"agilitate":3,"vitalitate":7,"noroc":2},"appearance":{"skin_tone":"olive","build":"sturdy","hair":"tied","accent":"gold"},"level":4,"xp":120,"unspent_points":3,"wallet":365,"owned_items":["BataCiobaneasca","Palos","ScutDeLemn"],"equipped":["Palos","ScutDeLemn"],"ladder_progress":12,"lap":2}"#
    }

    #[test]
    fn an_exact_v1_fixture_migrates_with_the_documented_defaults() {
        let migrated = SaveGame::from_json(exact_v1_fixture()).expect("v1 fixture migrates");
        assert_eq!(migrated.version, CURRENT_VERSION);
        // Every v1 field is carried over verbatim — no v1 field is lost.
        assert_eq!(migrated.name, "Făt-Frumos");
        assert_eq!(
            migrated.attrs,
            SavedAttributes {
                putere: 5,
                agilitate: 3,
                vitalitate: 7,
                noroc: 2,
            }
        );
        assert_eq!(
            migrated.appearance,
            PlayerAppearance {
                skin_tone: SkinTone::Olive,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Tied,
                accent: AccentColor::Gold,
            }
        );
        assert_eq!(migrated.level, 4);
        assert_eq!(migrated.xp, 120);
        assert_eq!(migrated.unspent_points, 3);
        assert_eq!(migrated.wallet, 365);
        assert_eq!(
            migrated.owned_items,
            vec!["BataCiobaneasca", "Palos", "ScutDeLemn"]
        );
        assert_eq!(migrated.equipped, vec!["Palos", "ScutDeLemn"]);
        assert_eq!(migrated.ladder_progress, 12);
        assert_eq!(migrated.lap, 2);
        // The two new v2 fields get their documented safe defaults.
        assert_eq!(
            migrated.lifetime_earnings, 365,
            "lifetime_earnings defaults to the v1 wallet balance"
        );
        assert_eq!(
            migrated.resume_destination,
            ResumeDestination::Fight,
            "resume_destination defaults to v1's only real behavior"
        );
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
    fn an_unknown_item_in_a_migrated_v1_save_discards_it_too() {
        let json = r#"{"version":1,"name":"x","attrs":{"putere":1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":["NuExista"],"equipped":[],"ladder_progress":0,"lap":1}"#;
        assert!(
            SaveGame::from_json(json).is_none(),
            "validation applies after migration, not just to native v2 saves"
        );
    }

    #[test]
    fn reset_restores_every_owned_resource_to_its_fresh_run_value() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(sample_save().player_character());
        app.insert_resource(Level {
            level: 9,
            xp: 40,
            unspent_points: 5,
        });
        app.insert_resource(Wallet(9_999));
        app.insert_resource(LifetimeEarnings(12_345));
        app.insert_resource(OwnedItems(HashSet::from([ItemId::Palos])));
        let mut equipment = Equipment::default();
        equipment.equip(ItemId::Palos);
        app.insert_resource(PlayerEquipment(equipment));
        app.insert_resource(LadderProgress(37));

        fn reset_system(mut commands: Commands) {
            reset(&mut commands);
        }
        app.add_systems(Update, reset_system);
        app.update();

        assert!(
            app.world().get_resource::<PlayerCharacter>().is_none(),
            "a fresh run has no confirmed hero yet"
        );
        assert_eq!(*app.world().resource::<Level>(), Level::default());
        assert_eq!(*app.world().resource::<Wallet>(), Wallet::default());
        assert_eq!(
            *app.world().resource::<LifetimeEarnings>(),
            LifetimeEarnings::default()
        );
        assert_eq!(*app.world().resource::<OwnedItems>(), OwnedItems::default());
        assert_eq!(
            *app.world().resource::<PlayerEquipment>(),
            PlayerEquipment::default()
        );
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress::default()
        );
    }
}
