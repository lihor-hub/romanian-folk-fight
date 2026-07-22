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
//! # v2 → v3 safe-default table (#128)
//!
//! v3 widens the attribute spread from four fields to eight
//! (`atac`/`aparare`/`carisma`/`magie` join, see
//! [`crate::character::Attributes`]).
//!
//! | v3 field | v2 source | migrated value | why it's safe |
//! |---|---|---|---|
//! | `attrs.atac` / `attrs.aparare` / `attrs.carisma` | *(new)* | 1 (their [`crate::character::AttributeKind::base_value`]) | exactly what a fresh hero starts with; never fabricates allocated points |
//! | `attrs.magie` | *(new)* | 0 (its base value) | `magie == 0` is a valid non-caster (zero mana, no starting spell) and is never normalized upward |
//! | `unspent_points` | `v2.unspent_points` | `+ `[`v3_widening_compensation_points`] | v2 heroes were built from a 10-point creation pool and 2 points per level; v3 widened those to [`crate::creation::FREE_POINTS`] / [`crate::progression::POINTS_PER_LEVEL`]. Granting the difference as *unspent* points lets a migrated hero re-spend into the new attributes and end up exactly as wide as a fresh v3 hero of the same level — without ever pre-allocating on the player's behalf |
//! | everything else | `v2.*` | carried over verbatim | no new information needed |
//!
//! # v3 → v4 safe-default table (#319)
//!
//! | v4 field | v3 source | migrated value | why it's safe |
//! |---|---|---|---|
//! | `definition` | `appearance` | [`crate::character::CharacterDefinition::legacy_human`]`(appearance)` | v3 player identity was fully represented by the legacy appearance controls; the adapter resolves those choices to stable human part IDs without changing the visible palette/proportions bridge |
//! | `appearance` | `v3.appearance` | carried over verbatim | existing UI and rendering consumers keep their compatibility projection while the definition becomes authoritative identity |
//! | everything else | `v3.*` | carried over verbatim | no other run state changes in v4 |
//!
//! # v4 → v5 safe-default table (#319)
//!
//! | v5 field | v4 source | migrated value | why it's safe |
//! |---|---|---|---|
//! | `campaign_seed` | *(new)* | [`crate::roster::CampaignSeed::default`] | this is the campaign seed older builds implicitly used for the representative ladder encounter |
//! | `seeded_opponent` | `ladder_progress` + migrated `campaign_seed` | the exact resolved representative opponent for that rung, or `None` where the modular tracer bullet does not apply | resolution happens once during migration and v5 persists the result, so later catalog/profile changes cannot silently change an existing encounter |
//! | everything else | `v4.*` | carried over verbatim | no other run state changes in v5 |
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

use crate::character::{Attributes, CharacterDefinition, PlayerAppearance};
use crate::creation::PlayerCharacter;
use crate::items::{Equipment, ItemId, Slot};
use crate::progression::{Level, LifetimeEarnings, Wallet};
use crate::roster::{CampaignSeed, LadderProgress, PreparedEncounter, SeededOpponent};
use crate::shop::{OwnedItems, PlayerEquipment};

/// The version written into every save produced by this build; loads of any
/// other value either migrate forward (if older and known, see [`Migrate`])
/// or are discarded (if unknown/newer).
pub const CURRENT_VERSION: u32 = 5;

/// Serde mirror of [`Attributes`] (eight attributes since v3/#128); the
/// character model stays serde-free.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedAttributes {
    pub putere: u32,
    pub agilitate: u32,
    pub vitalitate: u32,
    pub noroc: u32,
    pub atac: u32,
    pub aparare: u32,
    pub carisma: u32,
    pub magie: u32,
}

impl From<Attributes> for SavedAttributes {
    fn from(attrs: Attributes) -> Self {
        Self {
            putere: attrs.putere,
            agilitate: attrs.agilitate,
            vitalitate: attrs.vitalitate,
            noroc: attrs.noroc,
            atac: attrs.atac,
            aparare: attrs.aparare,
            carisma: attrs.carisma,
            magie: attrs.magie,
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
            atac: attrs.atac,
            aparare: attrs.aparare,
            carisma: attrs.carisma,
            magie: attrs.magie,
        }
    }
}

/// The four-attribute spread of v1/v2 payloads (pre-#128): kept only so
/// [`SnapshotV1`]/[`SnapshotV2`] can parse old saves for migration. Nothing
/// else constructs this.
#[derive(Deserialize, Debug, Clone, Copy)]
struct SavedAttributesV2 {
    putere: u32,
    agilitate: u32,
    vitalitate: u32,
    noroc: u32,
}

impl SavedAttributesV2 {
    /// Widens the four-attribute spread to eight, giving each new attribute
    /// its fresh-hero base value — see the v2 → v3 default table in the
    /// module docs.
    fn widen(self) -> SavedAttributes {
        let base = Attributes::default();
        SavedAttributes {
            putere: self.putere,
            agilitate: self.agilitate,
            vitalitate: self.vitalitate,
            noroc: self.noroc,
            atac: base.atac,
            aparare: base.aparare,
            carisma: base.carisma,
            magie: base.magie,
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
/// [`crate::flow::FlowIntent::ContinueRun`]/[`crate::flow::FlowIntent::ContinueToShop`]/
/// [`crate::flow::FlowIntent::ContinueToTown`] (never interpreted as a raw
/// field by any other screen -- see `crate::flow`'s ownership-boundary
/// docs).
///
/// #217 wires every safe checkpoint (hero confirmation, the result/reward
/// autosave, shop entry/purchases/equips, and the victory/lap autosave) to
/// pass the destination that actually matches where **Continuă** should
/// land, via [`SaveGame::capture`]'s explicit parameter -- there is no
/// implicit/default destination for a fresh capture, precisely so a new
/// checkpoint can never forget to pick one. #129 (the town hub) followed
/// exactly that recipe for [`ResumeDestination::Town`]: a new variant here
/// plus a new arm wherever `crate::menu` maps [`ResumeDestination`] to a
/// `FlowIntent` -- a future safe destination (a child of #133/#137/#140)
/// repeats it; see that module's doc comment for the exact extension steps.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResumeDestination {
    /// Resume straight into the next fight. Since #129 no checkpoint writes
    /// this anymore (the hero-confirmation and result/reward checkpoints
    /// moved to [`ResumeDestination::Town`]), but it stays the `#[default]`
    /// deliberately: it is the serde fallback for pre-#217 saves whose
    /// payload has no `resume_destination` field at all, and those must keep
    /// resuming into the arena exactly as they always have.
    #[default]
    Fight,
    /// Resume into the shop -- the destination for shop entry and every shop
    /// purchase/equip autosave (#217).
    Shop,
    /// Resume into the town hub (#129) -- the destination every non-shop
    /// checkpoint writes since the hub landed: hero confirmation, the
    /// result/reward and level-up autosaves, the victory/lap autosave, and
    /// the hub's own entry autosave. Additive: pre-#129 saves storing
    /// `"fight"`/`"shop"` keep resuming exactly where they said, and only
    /// saves newly written by a #129 build carry `"town"`.
    Town,
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
    attrs: SavedAttributesV2,
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

/// v2 payload (#193, superseded by #128's v3): kept only so
/// [`SaveGame::from_json`] can parse and [`Migrate`] old saves. Nothing else
/// in the codebase constructs this — new saves are always [`SaveGame`]. The
/// `#[serde(default)]` fields mirror the tolerances the v2-era `SaveGame`
/// itself had.
#[derive(Deserialize, Debug, Clone)]
struct SnapshotV2 {
    name: String,
    attrs: SavedAttributesV2,
    #[serde(default)]
    appearance: PlayerAppearance,
    level: u32,
    xp: u32,
    unspent_points: u32,
    wallet: u32,
    #[serde(default)]
    lifetime_earnings: u32,
    owned_items: Vec<String>,
    equipped: Vec<String>,
    ladder_progress: usize,
    lap: u32,
    #[serde(default)]
    resume_destination: ResumeDestination,
}

/// v3 payload (#128, superseded by #319's v4): kept only so
/// [`SaveGame::from_json`] can parse and [`Migrate`] old saves. The shape is
/// the v3 `SaveGame` verbatim, before stable resolved character definitions
/// became run-scoped save data.
#[derive(Deserialize, Debug, Clone)]
struct SnapshotV3 {
    name: String,
    attrs: SavedAttributes,
    #[serde(default)]
    appearance: PlayerAppearance,
    level: u32,
    xp: u32,
    unspent_points: u32,
    wallet: u32,
    #[serde(default)]
    lifetime_earnings: u32,
    owned_items: Vec<String>,
    equipped: Vec<String>,
    ladder_progress: usize,
    lap: u32,
    #[serde(default)]
    resume_destination: ResumeDestination,
}

/// The creation free-point pool a v2-era hero was built from — frozen
/// historical fact, deliberately *not* [`crate::creation::FREE_POINTS`]
/// (which later balance passes like #149 may move again).
const V2_CREATION_FREE_POINTS: u32 = 10;

/// The points-per-level a v2-era hero leveled with — frozen historical
/// fact, deliberately *not* [`crate::progression::POINTS_PER_LEVEL`].
const V2_POINTS_PER_LEVEL: u32 = 2;

/// How many extra *unspent* attribute points the v2 → v3 migration grants a
/// hero of `level`: the creation-pool widening plus the per-level widening
/// for every level-up the hero has already banked, computed against the
/// live v3 constants (so a later balance pass keeps migrated heroes exactly
/// as wide as fresh ones). See the v2 → v3 default table in the module docs.
fn v3_widening_compensation_points(level: u32) -> u32 {
    let creation_delta = crate::creation::FREE_POINTS.saturating_sub(V2_CREATION_FREE_POINTS);
    let per_level_delta = crate::progression::POINTS_PER_LEVEL.saturating_sub(V2_POINTS_PER_LEVEL);
    creation_delta + per_level_delta * level.saturating_sub(1)
}

/// Migrates one schema version's payload into the next. Each future version
/// (#133/#137/#140 — see this module's extension recipe) adds one impl of
/// this, chained by [`SaveGame::from_json`].
trait Migrate {
    type Next;
    fn migrate(self) -> Self::Next;
}

impl Migrate for SnapshotV1 {
    type Next = SnapshotV2;

    /// See the v1 → v2 default table in this module's docs for the
    /// rationale behind `lifetime_earnings` and `resume_destination`.
    fn migrate(self) -> SnapshotV2 {
        SnapshotV2 {
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

impl Migrate for SnapshotV2 {
    type Next = SnapshotV3;

    /// See the v2 → v3 default table in this module's docs: the four new
    /// attributes arrive at their fresh-hero base values (magie 0 — a valid
    /// non-caster, never normalized upward) and the pool widening lands as
    /// *unspent* points for the player to re-spend.
    fn migrate(self) -> SnapshotV3 {
        SnapshotV3 {
            name: self.name,
            attrs: self.attrs.widen(),
            appearance: self.appearance,
            level: self.level,
            xp: self.xp,
            unspent_points: self.unspent_points + v3_widening_compensation_points(self.level),
            wallet: self.wallet,
            lifetime_earnings: self.lifetime_earnings,
            owned_items: self.owned_items,
            equipped: self.equipped,
            ladder_progress: self.ladder_progress,
            lap: self.lap,
            resume_destination: self.resume_destination,
        }
    }
}

impl Migrate for SnapshotV3 {
    type Next = SnapshotV4;

    /// See the v3 → v4 default table in this module's docs: v3's appearance
    /// was its complete player-identity contract, so resolve that legacy
    /// projection to stable human part IDs while carrying the projection
    /// itself forward for current UI and rendering consumers.
    fn migrate(self) -> SnapshotV4 {
        SnapshotV4 {
            name: self.name,
            attrs: self.attrs,
            appearance: self.appearance,
            definition: Some(CharacterDefinition::legacy_human(self.appearance)),
            level: self.level,
            xp: self.xp,
            unspent_points: self.unspent_points,
            wallet: self.wallet,
            lifetime_earnings: self.lifetime_earnings,
            owned_items: self.owned_items,
            equipped: self.equipped,
            ladder_progress: self.ladder_progress,
            lap: self.lap,
            resume_destination: self.resume_destination,
        }
    }
}

/// v4 payload (#319 before resolved encounter provenance was persisted).
/// `definition` remained optional during v4 so additive v4 saves could
/// reconstruct the player's stable IDs from `appearance`.
#[derive(Deserialize, Debug, Clone)]
struct SnapshotV4 {
    name: String,
    attrs: SavedAttributes,
    #[serde(default)]
    appearance: PlayerAppearance,
    #[serde(default)]
    definition: Option<CharacterDefinition>,
    level: u32,
    xp: u32,
    unspent_points: u32,
    wallet: u32,
    #[serde(default)]
    lifetime_earnings: u32,
    owned_items: Vec<String>,
    equipped: Vec<String>,
    ladder_progress: usize,
    lap: u32,
    #[serde(default)]
    resume_destination: ResumeDestination,
}

impl Migrate for SnapshotV4 {
    type Next = SaveGame;

    fn migrate(self) -> SaveGame {
        let campaign_seed = CampaignSeed::default();
        let ladder = LadderProgress(self.ladder_progress);
        let seeded_opponent =
            ladder
                .seeded_opponent(campaign_seed)
                .and_then(|result| match result {
                    Ok(generated) => Some(Box::new(generated)),
                    Err(error) => {
                        warn!("could not migrate legacy encounter identity: {error}");
                        None
                    }
                });
        SaveGame {
            version: CURRENT_VERSION,
            name: self.name,
            attrs: self.attrs,
            appearance: self.appearance,
            definition: Box::new(
                self.definition
                    .unwrap_or_else(|| CharacterDefinition::legacy_human(self.appearance)),
            ),
            level: self.level,
            xp: self.xp,
            unspent_points: self.unspent_points,
            wallet: self.wallet,
            lifetime_earnings: self.lifetime_earnings,
            owned_items: self.owned_items,
            equipped: self.equipped,
            ladder_progress: self.ladder_progress,
            lap: self.lap,
            campaign_seed: campaign_seed.0,
            seeded_opponent,
            resume_destination: self.resume_destination,
        }
    }
}

/// One full run snapshot (v5, #319; envelope from #193): mirrors every
/// run-scoped resource — the confirmed character (eight attributes and stable
/// resolved identity), the experience state, the wallet and lifetime earnings,
/// the shop purchases and loadout, the ladder position, prepared encounter
/// provenance, and the typed safe resume destination.
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
    /// [`PlayerCharacter::definition`], including stable resolved part IDs.
    pub definition: Box<CharacterDefinition>,
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
    /// Campaign-level input used to derive generated encounter provenance.
    pub campaign_seed: u64,
    /// Exact resolved representative encounter identity. Stable IDs remain
    /// authoritative across catalog/profile changes after this snapshot.
    pub seeded_opponent: Option<Box<SeededOpponent>>,
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
        campaign_seed: CampaignSeed,
        prepared_encounter: Option<&PreparedEncounter>,
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
            definition: Box::new(player.definition.clone()),
            level: level.level,
            xp: level.xp,
            unspent_points: level.unspent_points,
            wallet: wallet.0,
            lifetime_earnings: lifetime_earnings.0,
            owned_items,
            equipped,
            ladder_progress: ladder.0,
            lap: ladder.lap(),
            campaign_seed: campaign_seed.0,
            seeded_opponent: prepared_encounter.map(|prepared| Box::new(prepared.0.clone())),
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
                .migrate()
                .migrate()
                .migrate()
                .migrate(),
            2 => serde_json::from_str::<SnapshotV2>(json)
                .map_err(|_| SnapshotLoadError::Invalid)?
                .migrate()
                .migrate()
                .migrate(),
            3 => serde_json::from_str::<SnapshotV3>(json)
                .map_err(|_| SnapshotLoadError::Invalid)?
                .migrate()
                .migrate(),
            4 => serde_json::from_str::<SnapshotV4>(json)
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
        if save.definition.version != crate::character::CHARACTER_DEFINITION_VERSION {
            warn!(
                "save contains unsupported character definition version {}; discarding",
                save.definition.version
            );
            return Err(SnapshotLoadError::Invalid);
        }
        if let Some(generated) = save.seeded_opponent.as_deref()
            && generated.definition.version != crate::character::CHARACTER_DEFINITION_VERSION
        {
            warn!(
                "save contains unsupported encounter definition version {}; discarding",
                generated.definition.version
            );
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
            definition: self.definition.as_ref().clone(),
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

    /// The saved campaign seed used for encounter provenance.
    pub fn campaign_seed(&self) -> CampaignSeed {
        CampaignSeed(self.campaign_seed)
    }

    /// The saved pre-resolved encounter identity, when this ladder rung has
    /// one in the modular tracer bullet.
    pub fn prepared_encounter(&self) -> Option<PreparedEncounter> {
        self.seeded_opponent
            .as_deref()
            .cloned()
            .map(PreparedEncounter)
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
        commands.insert_resource(self.campaign_seed());
        if let Some(prepared) = self.prepared_encounter() {
            commands.insert_resource(prepared);
        } else {
            commands.remove_resource::<PreparedEncounter>();
        }
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
    commands.insert_resource(CampaignSeed::default());
    commands.remove_resource::<PreparedEncounter>();
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::character::{
        AccentColor, BodyBuild, CharacterDefinition, HairStyle, PartId, SkinTone,
    };
    use crate::roster::{CampaignSeed, PreparedEncounter};

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
        let appearance = PlayerAppearance {
            skin_tone: SkinTone::Olive,
            build: BodyBuild::Sturdy,
            hair: HairStyle::Tied,
            accent: AccentColor::Gold,
        };
        let player = PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes {
                putere: 5,
                agilitate: 3,
                vitalitate: 7,
                noroc: 2,
                atac: 4,
                aparare: 3,
                carisma: 2,
                magie: 0,
            },
            appearance,
            definition: CharacterDefinition::legacy_human(appearance),
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
            CampaignSeed::default(),
            None,
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
                atac: 4,
                aparare: 3,
                carisma: 2,
                magie: 0,
            },
            "all eight attributes are captured, magie 0 included"
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
        assert_eq!(save.campaign_seed, CampaignSeed::default().0);
        assert!(save.seeded_opponent.is_none());
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

    /// #129: the town-hub destination round-trips the same way, and it is a
    /// purely additive variant -- it serializes as `"town"` without touching
    /// the save version.
    #[test]
    fn capture_stores_the_town_resume_destination_when_given_it() {
        let save = sample_save_with_destination(ResumeDestination::Town);
        assert_eq!(save.resume_destination, ResumeDestination::Town);
        assert_eq!(save.version, CURRENT_VERSION, "no version bump needed");
        let json = save.to_json().expect("plain data serializes");
        assert!(
            json.contains("\"resume_destination\":\"town\""),
            "additive snake_case wire value: {json}"
        );
        let restored = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(restored.resume_destination(), ResumeDestination::Town);
    }

    /// #129 compat proof: pre-town saves keep resuming exactly where they
    /// said. A stored `"fight"`/`"shop"` parses to its own variant, and a
    /// pre-#217 payload with no `resume_destination` field at all still
    /// falls back to `Fight` via `#[serde(default)]`.
    #[test]
    fn legacy_resume_destinations_still_parse_to_their_own_variants() {
        for (wire, expected) in [
            ("fight", ResumeDestination::Fight),
            ("shop", ResumeDestination::Shop),
            ("town", ResumeDestination::Town),
        ] {
            let mut json = sample_save().to_json().expect("plain data serializes");
            json = json.replace(
                "\"resume_destination\":\"fight\"",
                &format!("\"resume_destination\":\"{wire}\""),
            );
            let restored = SaveGame::from_json(&json).expect("stored payload loads");
            assert_eq!(restored.resume_destination(), expected, "{wire}");
        }

        let json = sample_save().to_json().expect("plain data serializes");
        let without_field = json.replace(",\"resume_destination\":\"fight\"", "");
        assert_ne!(json, without_field, "the field was present to remove");
        let restored =
            SaveGame::from_json(&without_field).expect("a payload missing the field still loads");
        assert_eq!(
            restored.resume_destination(),
            ResumeDestination::Fight,
            "missing field defaults to the pre-#217 arena resume"
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
    fn current_version_roundtrip_preserves_non_default_resolved_part_ids() {
        let (mut player, level, wallet, lifetime_earnings, owned, equipment, ladder) = sample_run();
        player.definition.parts.hair =
            PartId::new("human.hair.roundtrip-signature.v1").expect("test ID is valid");
        player.definition.parts.accessories =
            vec![PartId::new("human.accessory.roundtrip-talisman.v1").expect("test ID is valid")];
        let expected = player.definition.clone();

        let json = SaveGame::capture(
            &player,
            &level,
            &wallet,
            &lifetime_earnings,
            &owned,
            &equipment,
            &ladder,
            CampaignSeed::default(),
            None,
            ResumeDestination::Fight,
        )
        .to_json()
        .expect("plain data serializes");
        let restored = SaveGame::from_json(&json).expect("own JSON loads");

        assert_eq!(restored.player_character().definition, expected);
    }

    #[test]
    fn current_roundtrip_preserves_resolved_encounter_identity_and_campaign_seed() {
        let (player, level, wallet, lifetime_earnings, owned, equipment, ladder) = sample_run();
        let campaign_seed = CampaignSeed(93);
        let mut generated = LadderProgress(0)
            .seeded_opponent(campaign_seed)
            .expect("the representative encounter is generated")
            .expect("the bundled profile resolves");
        generated.definition.parts.hair =
            PartId::new("human.hair.persisted-signature.v1").expect("test ID is valid");
        let prepared = PreparedEncounter(generated.clone());

        let json = SaveGame::capture(
            &player,
            &level,
            &wallet,
            &lifetime_earnings,
            &owned,
            &equipment,
            &ladder,
            campaign_seed,
            Some(&prepared),
            ResumeDestination::Fight,
        )
        .to_json()
        .expect("plain data serializes");
        let restored = SaveGame::from_json(&json).expect("own JSON loads");

        assert_eq!(restored.campaign_seed(), campaign_seed);
        assert_eq!(restored.prepared_encounter(), Some(prepared));
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
            CampaignSeed::default(),
            None,
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
        assert_eq!(save.campaign_seed(), CampaignSeed::default());
        assert_eq!(save.prepared_encounter(), None);
        assert_eq!(save.resume_destination(), ResumeDestination::Fight);
    }

    #[test]
    fn corrupt_json_is_rejected_without_panicking() {
        for corrupt in [
            "",
            "not json at all",
            "{",
            "42",
            r#"{"version":4}"#,
            r#"{"version":3}"#,
            r#"{"version":2}"#,
            r#"{"version":1}"#,
            // Negative atac: fails to parse as the u32 the v3 shape expects.
            r#"{"version":3,"name":"x","attrs":{"putere":1,"agilitate":1,"vitalitate":1,"noroc":1,"atac":-1,"aparare":1,"carisma":1,"magie":0},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
            // A v3 payload missing the new attribute fields entirely: only
            // v1/v2 payloads may omit them (via migration); a payload
            // *claiming* v3 must carry all eight.
            r#"{"version":3,"name":"x","attrs":{"putere":1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
            // Negative putere in a v2 payload — the v2 parse-then-migrate
            // path must fail closed too, not just the current-version path.
            r#"{"version":2,"name":"x","attrs":{"putere":-1,"agilitate":1,"vitalitate":1,"noroc":1},"level":1,"xp":0,"unspent_points":0,"wallet":0,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1}"#,
            // Same corruption, but claiming to be a v1 payload — the v1
            // parse-then-migrate path must fail closed too.
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

    #[test]
    fn a_current_save_with_a_future_character_definition_is_rejected() {
        let mut save = sample_save();
        save.definition.version = crate::character::CHARACTER_DEFINITION_VERSION + 1;
        let json = save.to_json().expect("plain data serializes");

        assert_eq!(SaveGame::load(&json), Err(SnapshotLoadError::Invalid));
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
            r#"{"version":4}"#,
            r#"{"version":3}"#,
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
    fn an_exact_v1_fixture_migrates_through_the_whole_chain_to_v5() {
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
                atac: 1,
                aparare: 1,
                carisma: 1,
                magie: 0,
            },
            "the v3 widening gives every new attribute its base value"
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
        assert_eq!(
            migrated.unspent_points,
            3 + v3_widening_compensation_points(4),
            "the 3 banked v1 points plus the v3 pool-widening compensation"
        );
        assert_eq!(migrated.wallet, 365);
        assert_eq!(
            migrated.owned_items,
            vec!["BataCiobaneasca", "Palos", "ScutDeLemn"]
        );
        assert_eq!(migrated.equipped, vec!["Palos", "ScutDeLemn"]);
        assert_eq!(migrated.ladder_progress, 12);
        assert_eq!(migrated.lap, 2);
        // The two v2 fields get their documented safe defaults.
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

    /// The exact v2 fixture the v2 → v3 default table describes, shaped like
    /// a real pre-#128 capture (four-attribute spread, `lifetime_earnings`
    /// and `resume_destination` present).
    fn exact_v2_fixture() -> &'static str {
        r#"{"version":2,"name":"Făt-Frumos","attrs":{"putere":5,"agilitate":3,"vitalitate":7,"noroc":2},"appearance":{"skin_tone":"olive","build":"sturdy","hair":"tied","accent":"gold"},"level":4,"xp":120,"unspent_points":3,"wallet":365,"lifetime_earnings":510,"owned_items":["BataCiobaneasca","Palos","ScutDeLemn"],"equipped":["Palos","ScutDeLemn"],"ladder_progress":12,"lap":2,"resume_destination":"shop"}"#
    }

    #[test]
    fn an_exact_v3_fixture_reconstructs_the_legacy_human_definition() {
        let json = r#"{"version":3,"name":"Ileana Cosânzeana","attrs":{"putere":2,"agilitate":4,"vitalitate":3,"noroc":5,"atac":2,"aparare":3,"carisma":4,"magie":1},"appearance":{"skin_tone":"deep","build":"lean","hair":"long","accent":"storm"},"level":3,"xp":75,"unspent_points":2,"wallet":140,"lifetime_earnings":260,"owned_items":[],"equipped":[],"ladder_progress":7,"lap":1,"resume_destination":"fight"}"#;

        let migrated = SaveGame::from_json(json).expect("v3 fixture migrates");
        let player = migrated.player_character();

        assert_eq!(migrated.version, CURRENT_VERSION);
        assert_eq!(
            player.definition,
            CharacterDefinition::legacy_human(player.appearance)
        );
    }

    #[test]
    fn a_v4_payload_missing_definition_migrates_from_appearance() {
        let json = r#"{"version":4,"name":"Ileana Cosânzeana","attrs":{"putere":2,"agilitate":4,"vitalitate":3,"noroc":5,"atac":2,"aparare":3,"carisma":4,"magie":1},"appearance":{"skin_tone":"deep","build":"lean","hair":"long","accent":"storm"},"level":3,"xp":75,"unspent_points":2,"wallet":140,"lifetime_earnings":260,"owned_items":[],"equipped":[],"ladder_progress":7,"lap":1,"resume_destination":"fight"}"#;

        let loaded = SaveGame::from_json(json).expect("additive v4 payload loads");

        assert_eq!(
            loaded.definition.as_ref(),
            &CharacterDefinition::legacy_human(loaded.appearance)
        );
    }

    #[test]
    fn a_v4_representative_encounter_is_resolved_once_during_migration() {
        let json = r#"{"version":4,"name":"Ileana Cosânzeana","attrs":{"putere":2,"agilitate":4,"vitalitate":3,"noroc":5,"atac":2,"aparare":3,"carisma":4,"magie":1},"appearance":{"skin_tone":"deep","build":"lean","hair":"long","accent":"storm"},"level":3,"xp":75,"unspent_points":2,"wallet":140,"lifetime_earnings":260,"owned_items":[],"equipped":[],"ladder_progress":0,"lap":1,"resume_destination":"fight"}"#;
        let expected = LadderProgress(0)
            .seeded_opponent(CampaignSeed::default())
            .expect("the representative rung is modular")
            .expect("the bundled catalog resolves");

        let migrated = SaveGame::from_json(json).expect("v4 fixture migrates");

        assert_eq!(migrated.version, CURRENT_VERSION);
        assert_eq!(migrated.campaign_seed(), CampaignSeed::default());
        assert_eq!(
            migrated.prepared_encounter(),
            Some(PreparedEncounter(expected))
        );
    }

    #[test]
    fn an_exact_v2_fixture_migrates_with_the_documented_defaults() {
        let migrated = SaveGame::from_json(exact_v2_fixture()).expect("v2 fixture migrates");
        assert_eq!(migrated.version, CURRENT_VERSION);
        // Every v2 field is carried over verbatim — no v2 field is lost.
        assert_eq!(migrated.name, "Făt-Frumos");
        assert_eq!(migrated.level, 4);
        assert_eq!(migrated.xp, 120);
        assert_eq!(migrated.wallet, 365);
        assert_eq!(migrated.lifetime_earnings, 510);
        assert_eq!(migrated.ladder_progress, 12);
        assert_eq!(migrated.lap, 2);
        assert_eq!(
            migrated.resume_destination,
            ResumeDestination::Shop,
            "v2's own resume destination is preserved, not defaulted"
        );
        // The widened attributes get their documented base-value defaults.
        assert_eq!(
            migrated.attrs,
            SavedAttributes {
                putere: 5,
                agilitate: 3,
                vitalitate: 7,
                noroc: 2,
                atac: 1,
                aparare: 1,
                carisma: 1,
                magie: 0,
            }
        );
        // The pool widening lands as unspent points: creation 16 - 10 = 6,
        // plus (3 - 2) per banked level-up (level 4 → 3 of them).
        assert_eq!(migrated.unspent_points, 3 + 6 + 3);
        assert_eq!(v3_widening_compensation_points(4), 9);
        // The migrated hero is a valid non-caster until the player says
        // otherwise: zero mana, magie never normalized upward.
        let attrs: Attributes = migrated.attrs.into();
        assert_eq!(attrs.magie, 0);
        assert_eq!(crate::character::stats::max_mana(&attrs), 0);
    }

    /// A migrated hero must end up exactly as wide as a fresh v3 hero of the
    /// same level: spendable total (spread + unspent) equals base + creation
    /// pool + level-ups.
    #[test]
    fn a_migrated_hero_matches_a_fresh_v3_heros_point_budget() {
        use crate::character::AttributeKind;
        use crate::creation::FREE_POINTS;
        use crate::progression::POINTS_PER_LEVEL;

        let migrated = SaveGame::from_json(exact_v2_fixture()).expect("v2 fixture migrates");
        let attrs: Attributes = migrated.attrs.into();
        // The v2 hero had spent its whole 10-point creation pool plus
        // 2 * 3 level-up points, minus the 3 still banked.
        let fresh_budget =
            AttributeKind::base_total() + FREE_POINTS + POINTS_PER_LEVEL * (migrated.level - 1);
        assert_eq!(
            attrs.total() + migrated.unspent_points,
            fresh_budget,
            "no points fabricated, none lost"
        );
    }

    /// A level-1 v2 save gets only the creation-pool widening (no banked
    /// level-ups to compensate).
    #[test]
    fn the_widening_compensation_is_level_aware() {
        assert_eq!(v3_widening_compensation_points(1), 6);
        assert_eq!(v3_widening_compensation_points(2), 7);
        assert_eq!(
            v3_widening_compensation_points(0),
            6,
            "a (never produced) level-0 payload saturates instead of underflowing"
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
        app.insert_resource(CampaignSeed(999));
        let prepared = LadderProgress(0)
            .seeded_opponent(CampaignSeed(999))
            .expect("the representative rung is modular")
            .expect("the bundled catalog resolves");
        app.insert_resource(PreparedEncounter(prepared));

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
        assert_eq!(
            *app.world().resource::<CampaignSeed>(),
            CampaignSeed::default()
        );
        assert!(app.world().get_resource::<PreparedEncounter>().is_none());
    }
}
