//! Turn-based combat: a pure, seeded-RNG resolution core ([`engine`]), a
//! pure enemy decision policy ([`ai`]), a thin ECS layer ([`systems`]) that
//! connects them to the arena fighters, the fight-screen HUD ([`hud`]), and
//! the in-fight pause overlay ([`pause`]).

pub mod action_palette;
pub mod actions;
pub mod ai;
pub mod engine;
pub mod hud;
pub mod pause;
pub mod position;
pub mod systems;

pub use actions::{
    ActionCategory, ActionCost, ActionDescriptor, ActionId, DescriptorContext, ExtraDescriptors,
    generate_action_descriptors,
};
pub use ai::{AiProfile, choose_action, choose_action_at_separation};
pub use engine::{CombatAction, CombatEvent, FighterState};
pub use hud::CombatLog;
pub use pause::PauseState;
pub use position::{ArenaBounds, CombatSide, DuelPositions};
pub use systems::{CombatLogEvent, CombatPlugin, CombatRng, CombatTurn};
