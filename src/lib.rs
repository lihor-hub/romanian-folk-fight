//! Romanian Folk Fight — a turn-based arena RPG with Romanian folklore characters.
//!
//! Game logic lives in this library so it is unit-testable; the binary in
//! `main.rs` only configures the window and adds [`GamePlugin`].

pub mod core;

use bevy::prelude::*;

/// Top-level plugin that wires up every feature plugin of the game.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(core::CorePlugin);
    }
}
