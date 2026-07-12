//! Action descriptor contract (#189, a child of #143): the desktop combat
//! palette ([`super::action_palette`]) renders and behaves entirely from
//! [`ActionDescriptor`]s produced by [`generate_action_descriptors`] instead
//! of the HUD hard-coding a seven-button list.
//!
//! [`generate_action_descriptors`] is the *one* generator the issue requires:
//! it walks [`ALL_ACTIONS`] (the seven current [`CombatAction`] variants) and
//! builds one descriptor per action, deriving every state-dependent field —
//! legality, cost, chance, the disabled reason — by calling into the
//! existing engine/HUD rules ([`CombatAction::stamina_cost`],
//! [`DuelDistance::in_melee_reach`]/[`DuelDistance::band`],
//! [`stats::hit_percent`], [`action_disabled_reason`]) rather than forking
//! them. [`action_enabled`] itself is now defined *in terms of*
//! [`action_disabled_reason`] so there is exactly one source of truth for
//! "can this action run right now."
//!
//! ## Extensibility (#189's acceptance criterion)
//!
//! [`ExtraDescriptors`] is a small, always-present (default empty) resource
//! the desktop palette appends after the seven generated descriptors, before
//! building buttons. It is not `cfg(test)`-gated (Bevy system parameters
//! cannot easily be conditionally compiled per-build without duplicating the
//! system), but it costs nothing when empty and no production code ever
//! populates it — see `action_palette`'s own test module for the proof that
//! inserting one entry renders and emits an eighth button without a single
//! edit to the palette's layout code. A later *real* action (#199/#213 —
//! ranged attacks, spells, consumables, taunt/shove) extends
//! [`generate_action_descriptors`] itself (its own real combat semantics
//! belong in the engine, not this test seam).

use crate::character::{Attributes, stats};

use super::engine::{
    CombatAction, DuelDistance, HEAVY_STRIKE_BASE_HIT, QUICK_STRIKE_BASE_HIT, REST_RESTORE,
};
use super::systems::{CombatSide, CombatTurn};

/// Stable, kebab-case identifier for one action descriptor — what
/// registration/lookup keys off of (not the `CombatAction` enum directly, so
/// a future descriptor without a `CombatAction` counterpart could still
/// register), and the exact string [`ActionDescriptor::pictogram_id`]
/// reuses as the contract #122's pictogram art keys off of (e.g. a future
/// `assets/ui/pictograms/<id>.png`).
pub type ActionId = &'static str;

/// The small, closed vocabulary of action categories every descriptor
/// belongs to. Desktop (#189, this module) does not group by category — it
/// is a flat strip — but phone category disclosure (a later #143 child) and
/// future registrations (#199/#213: ranged attacks join `Strikes`; taunt/
/// shove, spells, and consumables join `Special`) both need this field to
/// already exist on every descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    /// Damage actions: quick/heavy strike today, ranged attacks later.
    Strikes,
    /// Guard/mitigation actions: block.
    Defense,
    /// Distance-changing actions: step forward/back, leap forward.
    Movement,
    /// Recovery and non-damage utility: rest.
    Utility,
    /// Reserved for later registrations: taunt/shove, spells, consumables.
    Special,
}

/// One resource an action can cost (or restore). Only [`ActionCost::Stamina`]
/// and [`ActionCost::Restore`] are used by the current seven actions
/// (movement costs [`ActionCost::None`] — a position change, not a resource
/// spend); [`ActionCost::Mana`]/[`ActionCost::Item`] exist so a later spell
/// or consumable (#199/#213) can register without extending this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCost {
    /// Spends `n` stamina.
    Stamina(i32),
    /// Restores `n` stamina (Rest).
    Restore(i32),
    /// Spends `n` mana (unused today; reserved for spells).
    Mana(i32),
    /// Spends `n` of a consumable item (unused today; reserved for
    /// consumables).
    Item(i32),
    /// No resource cost — a position change (movement).
    None,
}

impl ActionCost {
    /// The cost line shown under an action button's label — the same text
    /// [`super::hud::cost_label`] produced before #189, now derived from the
    /// structured cost instead of matching on [`CombatAction`] a second
    /// time.
    pub fn display_text(self) -> String {
        match self {
            Self::Stamina(n) => format!("-{n} stamina"),
            Self::Restore(n) => format!("+{n} stamina"),
            Self::Mana(n) => format!("-{n} mana"),
            Self::Item(n) => format!("-{n}"),
            Self::None => "poziție".to_string(),
        }
    }
}

/// View data for one combat action, entirely derived from current
/// duel/fighter/presentation state by [`generate_action_descriptors`]. The
/// desktop palette renders one button per descriptor and reads every visual
/// and behavioral fact (label, cost text, enabled state, disabled reason,
/// emitted intent) from here — it never hard-codes per-action logic.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionDescriptor {
    /// Stable kebab-case id (see [`ActionId`]).
    pub id: ActionId,
    /// The small category vocabulary this action belongs to.
    pub category: ActionCategory,
    /// Romanian button label.
    pub label: &'static str,
    /// String contract for #122's future pictogram art — currently equal to
    /// [`Self::id`] (no art ships in this issue).
    pub pictogram_id: ActionId,
    /// Stamina/mana/item cost (or restore), where applicable.
    pub cost: ActionCost,
    /// Percent chance to hit, for strikes; `None` for actions with no roll
    /// (block, rest, movement).
    pub hit_chance: Option<i32>,
    /// Whether the action is legal at the current duel distance, ignoring
    /// stamina/turn-order (e.g. a melee strike out of reach, or a movement
    /// action already at its distance bound).
    pub position_legal: bool,
    /// Whether selecting this action right now would actually emit a
    /// command — the same rule [`action_enabled`] exposes, restated here so
    /// callers never need to re-derive it. `disabled_reason.is_none()` iff
    /// this is `true`.
    pub enabled: bool,
    /// A player-readable, specific Romanian reason the action cannot be
    /// selected right now, or `None` when [`Self::enabled`] is `true`.
    pub disabled_reason: Option<String>,
    /// The existing combat command this descriptor emits when selected —
    /// never a forked/parallel command, always a real [`CombatAction`] the
    /// engine already resolves.
    pub intent: CombatAction,
}

/// The seven current combat actions, in the order the desktop palette
/// renders them (unchanged from the pre-#189 HUD's button order).
pub const ALL_ACTIONS: [CombatAction; 7] = [
    CombatAction::QuickStrike,
    CombatAction::HeavyStrike,
    CombatAction::Block,
    CombatAction::Rest,
    CombatAction::StepForward,
    CombatAction::StepBack,
    CombatAction::LeapForward,
];

/// Everything [`generate_action_descriptors`] needs to derive every
/// descriptor field, gathered by the ECS glue in `action_palette` from the
/// same components/resources the combat systems already read
/// ([`CombatTurn`], the player's `Stamina`, both fighters' `Attributes`, and
/// [`super::systems::CombatPresentation`]). Kept as a plain, `Copy` struct so
/// descriptor generation stays a pure function, unit-testable without a
/// Bevy `World`.
#[derive(Debug, Clone, Copy)]
pub struct DescriptorContext {
    pub turn: CombatTurn,
    pub player_stamina: i32,
    pub player_attributes: Attributes,
    pub enemy_attributes: Attributes,
    pub presentation_busy: bool,
}

impl DescriptorContext {
    /// A permissive context for the action bar's very first (cosmetic)
    /// spawn, before [`CombatTurn`] exists: `combat::systems::init_turn`
    /// only inserts it once both fighters are queryable, so
    /// `action_palette::spawn_action_bar` (which runs in the same
    /// `OnEnter(GameState::Fight)` batch as the arena's fighter spawn) has
    /// no real duel state to read yet. This mirrors the pre-#189 HUD
    /// exactly: every button spawned showing its cost line and no
    /// [`crate::menu::DisabledButton`] marker, corrected on the very next
    /// frame once `update_action_buttons` runs with real state (`combat::systems`'s
    /// `Update` schedule chains `init_turn` before every HUD system, so
    /// `CombatTurn` is always present by then). `action_palette` never
    /// renders this context's `enabled`/`disabled_reason` fields at spawn —
    /// only `id`/`category`/`label`/`pictogram_id`/`cost` — so its exact
    /// distance/stamina values are inert placeholders, not a claim about
    /// the real duel.
    pub fn spawn_placeholder() -> Self {
        Self {
            turn: CombatTurn {
                side: CombatSide::Player,
                over: false,
                player_blocking: false,
                enemy_blocking: false,
                distance: DuelDistance::starting(),
            },
            player_stamina: i32::MAX,
            player_attributes: Attributes::default(),
            enemy_attributes: Attributes::default(),
            presentation_busy: false,
        }
    }
}

/// Registered descriptor extensions beyond [`ALL_ACTIONS`]. Always present
/// (default empty) via [`super::systems::CombatPlugin`]'s
/// `init_resource::<ExtraDescriptors>()`; see the module docs for why this —
/// not a `cfg(test)` item — is #189's "test-registered" extensibility proof.
#[derive(bevy::prelude::Resource, Debug, Clone, Default)]
pub struct ExtraDescriptors(pub Vec<ActionDescriptor>);

/// The Romanian button label for an action. Unchanged from the pre-#189
/// `hud::action_label`.
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

/// The stamina-cost line under a button label — kept for the pre-#189 call
/// sites/tests; equivalent to `action_cost(action).display_text()`.
pub fn cost_label(action: CombatAction) -> String {
    action_cost(action).display_text()
}

/// The stable kebab-case id for an action — see [`ActionId`].
pub fn action_id(action: CombatAction) -> ActionId {
    match action {
        CombatAction::QuickStrike => "quick-strike",
        CombatAction::HeavyStrike => "heavy-strike",
        CombatAction::Block => "block",
        CombatAction::Rest => "rest",
        CombatAction::StepForward => "step-forward",
        CombatAction::StepBack => "step-back",
        CombatAction::LeapForward => "leap-forward",
    }
}

/// The category an action belongs to — see [`ActionCategory`].
pub fn action_category(action: CombatAction) -> ActionCategory {
    match action {
        CombatAction::QuickStrike | CombatAction::HeavyStrike => ActionCategory::Strikes,
        CombatAction::Block => ActionCategory::Defense,
        CombatAction::Rest => ActionCategory::Utility,
        CombatAction::StepForward | CombatAction::StepBack | CombatAction::LeapForward => {
            ActionCategory::Movement
        }
    }
}

/// The structured cost/restore an action carries — see [`ActionCost`]. Reads
/// [`CombatAction::stamina_cost`] and [`REST_RESTORE`], the engine's own cost
/// table, rather than restating the numbers.
pub fn action_cost(action: CombatAction) -> ActionCost {
    match action {
        CombatAction::QuickStrike | CombatAction::HeavyStrike | CombatAction::Block => {
            ActionCost::Stamina(action.stamina_cost())
        }
        CombatAction::Rest => ActionCost::Restore(REST_RESTORE),
        CombatAction::StepForward | CombatAction::StepBack | CombatAction::LeapForward => {
            ActionCost::None
        }
    }
}

/// Whether `action` is legal at `distance`, ignoring stamina/turn-order —
/// reads [`DuelDistance::in_melee_reach`]/[`DuelDistance::band`] directly,
/// the same primitives `combat::engine::resolve_action_at_distance` itself
/// gates on, so this can never drift from the resolver's own reach rules.
fn position_legal(action: CombatAction, distance: DuelDistance) -> bool {
    match action {
        CombatAction::QuickStrike | CombatAction::HeavyStrike => distance.in_melee_reach(),
        CombatAction::Block | CombatAction::Rest => true,
        CombatAction::StepForward | CombatAction::LeapForward => {
            distance.band() > DuelDistance::CLOSE.band()
        }
        CombatAction::StepBack => distance.band() < DuelDistance::FAR.band(),
    }
}

/// The percent chance to hit, for strikes — calls
/// [`stats::hit_percent`] with the same base hit constants
/// (`QUICK_STRIKE_BASE_HIT`/`HEAVY_STRIKE_BASE_HIT`)
/// `combat::engine::resolve_action_at_distance` itself rolls against, so the
/// descriptor's number is always the number the engine would actually roll.
/// `None` for actions with no hit roll.
fn hit_chance(action: CombatAction, attacker: &Attributes, defender: &Attributes) -> Option<i32> {
    match action {
        CombatAction::QuickStrike => Some(stats::hit_percent(
            attacker,
            defender,
            QUICK_STRIKE_BASE_HIT,
        )),
        CombatAction::HeavyStrike => Some(stats::hit_percent(
            attacker,
            defender,
            HEAVY_STRIKE_BASE_HIT,
        )),
        CombatAction::Block | CombatAction::Rest => None,
        CombatAction::StepForward | CombatAction::StepBack | CombatAction::LeapForward => None,
    }
}

/// A player-readable, specific Romanian reason `action` cannot be selected
/// right now, or `None` when it can. [`action_enabled`] is defined in terms
/// of this function's result, so there is exactly one source of truth for
/// action legality — this function *is* the engine-matching rule the
/// pre-#189 `hud::action_enabled` documented, not a second copy of it.
pub fn action_disabled_reason(
    turn: &CombatTurn,
    stamina: i32,
    presentation_busy: bool,
    action: CombatAction,
) -> Option<String> {
    if presentation_busy {
        return Some("Se așteaptă finalizarea acțiunii precedente.".to_string());
    }
    if turn.over {
        return Some("Lupta s-a încheiat.".to_string());
    }
    if turn.side != CombatSide::Player {
        return Some("Nu e rândul tău.".to_string());
    }
    match action {
        CombatAction::QuickStrike | CombatAction::HeavyStrike => {
            if !turn.distance.in_melee_reach() {
                return Some("Prea departe pentru lovitură.".to_string());
            }
            if stamina < action.stamina_cost() {
                return Some(format!(
                    "Stamina insuficientă (nevoie {}).",
                    action.stamina_cost()
                ));
            }
            None
        }
        CombatAction::Block | CombatAction::Rest => None,
        CombatAction::StepForward | CombatAction::LeapForward => {
            if turn.distance.band() <= DuelDistance::CLOSE.band() {
                Some("Ești deja aproape.".to_string())
            } else {
                None
            }
        }
        CombatAction::StepBack => {
            if turn.distance.band() >= DuelDistance::FAR.band() {
                Some("Ești deja la distanță maximă.".to_string())
            } else {
                None
            }
        }
    }
}

/// Whether an action button is clickable, matching the engine's rules
/// exactly. Equivalent to `action_disabled_reason(..).is_none()` — kept as
/// a separate `bool`-returning function because it reads better at most call
/// sites (and is the pre-#189 `hud::action_enabled`'s exact signature/
/// behavior, so nothing else in the HUD needed to change).
pub fn action_enabled(
    turn: &CombatTurn,
    stamina: i32,
    presentation_busy: bool,
    action: CombatAction,
) -> bool {
    action_disabled_reason(turn, stamina, presentation_busy, action).is_none()
}

/// Builds one descriptor for `action` from `ctx` — every field derived by
/// calling into the functions above, never re-implemented inline.
fn descriptor_for(action: CombatAction, ctx: &DescriptorContext) -> ActionDescriptor {
    let disabled_reason =
        action_disabled_reason(&ctx.turn, ctx.player_stamina, ctx.presentation_busy, action);
    let id = action_id(action);
    ActionDescriptor {
        id,
        category: action_category(action),
        label: action_label(action),
        pictogram_id: id,
        cost: action_cost(action),
        hit_chance: hit_chance(action, &ctx.player_attributes, &ctx.enemy_attributes),
        position_legal: position_legal(action, ctx.turn.distance),
        enabled: disabled_reason.is_none(),
        disabled_reason,
        intent: action,
    }
}

/// The one descriptor generator #189 requires: produces all seven current
/// actions' descriptors from `ctx`, deriving legality/cost/chance from the
/// existing engine/HUD rules. `action_palette` appends
/// [`ExtraDescriptors`] after this — registering a later real action means
/// adding it to [`ALL_ACTIONS`] (and the small per-action match arms above),
/// never touching `action_palette`'s rendering code.
pub fn generate_action_descriptors(ctx: &DescriptorContext) -> Vec<ActionDescriptor> {
    ALL_ACTIONS
        .iter()
        .map(|&action| descriptor_for(action, ctx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::stats::{HIT_PERCENT_MAX, HIT_PERCENT_MIN};

    const PLAYER_TURN: CombatTurn = CombatTurn {
        side: CombatSide::Player,
        over: false,
        player_blocking: false,
        enemy_blocking: false,
        distance: DuelDistance::CLOSE,
    };

    fn ctx(turn: CombatTurn, stamina: i32) -> DescriptorContext {
        DescriptorContext {
            turn,
            player_stamina: stamina,
            player_attributes: Attributes {
                putere: 4,
                agilitate: 2,
                vitalitate: 4,
                noroc: 3,
            },
            enemy_attributes: Attributes {
                putere: 2,
                agilitate: 2,
                vitalitate: 2,
                noroc: 1,
            },
            presentation_busy: false,
        }
    }

    // --- id / category / label / pictogram coverage ---

    #[test]
    fn every_action_has_a_unique_stable_id() {
        let ids: Vec<ActionId> = ALL_ACTIONS.iter().map(|&a| action_id(a)).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "every action id must be unique");
        for id in ids {
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{id} must be kebab-case ascii"
            );
        }
    }

    #[test]
    fn generate_action_descriptors_produces_all_seven_current_actions() {
        let descriptors = generate_action_descriptors(&ctx(PLAYER_TURN, 50));
        assert_eq!(descriptors.len(), 7);
        for action in ALL_ACTIONS {
            assert!(
                descriptors.iter().any(|d| d.intent == action),
                "{action:?} must be produced by the one generator"
            );
        }
    }

    #[test]
    fn pictogram_id_equals_the_stable_id_for_every_action() {
        for descriptor in generate_action_descriptors(&ctx(PLAYER_TURN, 50)) {
            assert_eq!(descriptor.pictogram_id, descriptor.id);
        }
    }

    #[test]
    fn categories_match_the_documented_vocabulary() {
        let cases = [
            (CombatAction::QuickStrike, ActionCategory::Strikes),
            (CombatAction::HeavyStrike, ActionCategory::Strikes),
            (CombatAction::Block, ActionCategory::Defense),
            (CombatAction::Rest, ActionCategory::Utility),
            (CombatAction::StepForward, ActionCategory::Movement),
            (CombatAction::StepBack, ActionCategory::Movement),
            (CombatAction::LeapForward, ActionCategory::Movement),
        ];
        for (action, expected) in cases {
            assert_eq!(action_category(action), expected, "{action:?}");
        }
    }

    // --- labels / cost text (pre-#189 hud tests, moved here) ---

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

    #[test]
    fn action_cost_matches_the_engine_stamina_table() {
        assert_eq!(
            action_cost(CombatAction::QuickStrike),
            ActionCost::Stamina(CombatAction::QuickStrike.stamina_cost())
        );
        assert_eq!(
            action_cost(CombatAction::HeavyStrike),
            ActionCost::Stamina(CombatAction::HeavyStrike.stamina_cost())
        );
        assert_eq!(
            action_cost(CombatAction::Block),
            ActionCost::Stamina(CombatAction::Block.stamina_cost())
        );
        assert_eq!(
            action_cost(CombatAction::Rest),
            ActionCost::Restore(REST_RESTORE)
        );
        for movement in [
            CombatAction::StepForward,
            CombatAction::StepBack,
            CombatAction::LeapForward,
        ] {
            assert_eq!(action_cost(movement), ActionCost::None, "{movement:?}");
        }
    }

    // --- action_enabled / action_disabled_reason (pre-#189 hud test, moved
    // and extended with reason coverage) ---

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
            assert_eq!(
                action_disabled_reason(&turn, stamina, false, action).is_none(),
                expected,
                "disabled_reason must agree with action_enabled: {why}"
            );
        }
        assert!(
            !action_enabled(&PLAYER_TURN, 50, true, QuickStrike),
            "presentation busy disables otherwise-valid actions"
        );
    }

    #[test]
    fn disabled_reasons_are_specific_and_in_romanian() {
        let far = CombatTurn {
            distance: DuelDistance::FAR,
            ..PLAYER_TURN
        };
        let cases = [
            (
                far,
                20,
                CombatAction::QuickStrike,
                "Prea departe pentru lovitură.",
            ),
            (
                PLAYER_TURN,
                4,
                CombatAction::QuickStrike,
                "Stamina insuficientă (nevoie 5).",
            ),
            (
                PLAYER_TURN,
                0,
                CombatAction::StepForward,
                "Ești deja aproape.",
            ),
            (
                far,
                0,
                CombatAction::StepBack,
                "Ești deja la distanță maximă.",
            ),
        ];
        for (turn, stamina, action, expected) in cases {
            assert_eq!(
                action_disabled_reason(&turn, stamina, false, action),
                Some(expected.to_string()),
                "{action:?}"
            );
        }
        assert_eq!(
            action_disabled_reason(&PLAYER_TURN, 50, true, CombatAction::QuickStrike),
            Some("Se așteaptă finalizarea acțiunii precedente.".to_string())
        );
        let enemy_turn = CombatTurn {
            side: CombatSide::Enemy,
            ..PLAYER_TURN
        };
        assert_eq!(
            action_disabled_reason(&enemy_turn, 50, false, CombatAction::QuickStrike),
            Some("Nu e rândul tău.".to_string())
        );
        let over = CombatTurn {
            over: true,
            ..PLAYER_TURN
        };
        assert_eq!(
            action_disabled_reason(&over, 50, false, CombatAction::Rest),
            Some("Lupta s-a încheiat.".to_string())
        );
    }

    #[test]
    fn enabled_descriptors_never_carry_a_disabled_reason() {
        for descriptor in generate_action_descriptors(&ctx(PLAYER_TURN, 50)) {
            assert_eq!(
                descriptor.enabled,
                descriptor.disabled_reason.is_none(),
                "{}",
                descriptor.id
            );
        }
    }

    // --- position legality ---

    #[test]
    fn position_legal_matches_distance_gated_actions() {
        let close = DuelDistance::CLOSE;
        let far = DuelDistance::FAR;
        assert!(position_legal(CombatAction::QuickStrike, close));
        assert!(!position_legal(CombatAction::QuickStrike, far));
        assert!(position_legal(CombatAction::HeavyStrike, close));
        assert!(!position_legal(CombatAction::HeavyStrike, far));
        assert!(position_legal(CombatAction::Block, far));
        assert!(position_legal(CombatAction::Rest, far));
        assert!(!position_legal(CombatAction::StepForward, close));
        assert!(position_legal(CombatAction::StepForward, far));
        assert!(!position_legal(CombatAction::LeapForward, close));
        assert!(position_legal(CombatAction::LeapForward, far));
        assert!(position_legal(CombatAction::StepBack, close));
        assert!(!position_legal(CombatAction::StepBack, far));
    }

    #[test]
    fn descriptor_position_legal_field_matches_the_free_function() {
        for distance in [DuelDistance::CLOSE, DuelDistance::NEAR, DuelDistance::FAR] {
            let turn = CombatTurn {
                distance,
                ..PLAYER_TURN
            };
            for descriptor in generate_action_descriptors(&ctx(turn, 50)) {
                assert_eq!(
                    descriptor.position_legal,
                    position_legal(descriptor.intent, distance),
                    "{} at {distance:?}",
                    descriptor.id
                );
            }
        }
    }

    // --- hit chance ---

    #[test]
    fn strikes_carry_a_hit_chance_matching_the_engine_stats_formula() {
        let attacker = Attributes {
            putere: 4,
            agilitate: 2,
            vitalitate: 4,
            noroc: 3,
        };
        let defender = Attributes {
            putere: 2,
            agilitate: 2,
            vitalitate: 2,
            noroc: 1,
        };
        assert_eq!(
            hit_chance(CombatAction::QuickStrike, &attacker, &defender),
            Some(stats::hit_percent(
                &attacker,
                &defender,
                QUICK_STRIKE_BASE_HIT
            ))
        );
        assert_eq!(
            hit_chance(CombatAction::HeavyStrike, &attacker, &defender),
            Some(stats::hit_percent(
                &attacker,
                &defender,
                HEAVY_STRIKE_BASE_HIT
            ))
        );
    }

    #[test]
    fn non_strikes_carry_no_hit_chance() {
        for action in [
            CombatAction::Block,
            CombatAction::Rest,
            CombatAction::StepForward,
            CombatAction::StepBack,
            CombatAction::LeapForward,
        ] {
            let descriptor = descriptor_for(action, &ctx(PLAYER_TURN, 50));
            assert_eq!(descriptor.hit_chance, None, "{action:?}");
        }
    }

    #[test]
    fn hit_chance_is_clamped_to_the_engine_bounds() {
        let attacker = Attributes {
            putere: 1,
            agilitate: 1,
            vitalitate: 1,
            noroc: 100,
        };
        let defender = Attributes {
            putere: 1,
            agilitate: 100,
            vitalitate: 1,
            noroc: 1,
        };
        let descriptor = descriptor_for(
            CombatAction::QuickStrike,
            &ctx(PLAYER_TURN, 50).with_attrs(attacker, defender),
        );
        let chance = descriptor.hit_chance.expect("strikes carry a hit chance");
        assert!((HIT_PERCENT_MIN..=HIT_PERCENT_MAX).contains(&chance));
    }

    impl DescriptorContext {
        fn with_attrs(mut self, player: Attributes, enemy: Attributes) -> Self {
            self.player_attributes = player;
            self.enemy_attributes = enemy;
            self
        }
    }

    // --- extensibility seam ---

    #[test]
    fn extra_descriptors_resource_defaults_to_empty() {
        assert_eq!(ExtraDescriptors::default().0.len(), 0);
    }
}
