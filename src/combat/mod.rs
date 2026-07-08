//! Turn-based combat: a pure, seeded-RNG resolution core ([`engine`]), a
//! pure enemy decision policy ([`ai`]), a thin ECS layer ([`systems`]) that
//! connects them to the arena fighters, the fight-screen HUD ([`hud`]), and
//! the in-fight pause overlay ([`pause`]).

pub mod ai;
pub mod engine;
pub mod hud;
pub mod pause;
pub mod systems;

pub use ai::{AiProfile, choose_action, choose_action_at_distance};
pub use engine::{CombatAction, CombatEvent, DuelDistance, FighterState};
pub use hud::CombatLog;
pub use pause::PauseState;
pub use systems::{CombatLogEvent, CombatPlugin, CombatRng, CombatSide, CombatTurn};
