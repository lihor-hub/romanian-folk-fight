//! Turn-based combat: a pure, seeded-RNG resolution core ([`engine`]), a
//! pure enemy decision policy ([`ai`]), a thin ECS layer ([`systems`]) that
//! connects them to the arena fighters, and the fight-screen HUD ([`hud`]).

pub mod ai;
pub mod engine;
pub mod hud;
pub mod systems;

pub use ai::{AiProfile, choose_action};
pub use engine::{CombatAction, CombatEvent, FighterState};
pub use hud::CombatLog;
pub use systems::{CombatLogEvent, CombatPlugin, CombatRng, CombatSide, CombatTurn};
