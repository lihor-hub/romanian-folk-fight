//! Turn-based combat: a pure, seeded-RNG resolution core ([`engine`]) plus a
//! thin ECS layer ([`systems`]) that connects it to the arena fighters.

pub mod engine;
pub mod systems;

pub use engine::{CombatAction, CombatEvent, FighterState};
pub use systems::{CombatLogEvent, CombatPlugin, CombatRng, CombatSide, CombatTurn};
