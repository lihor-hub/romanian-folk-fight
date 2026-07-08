//! Romanian Folk Fight — a turn-based arena RPG with Romanian folklore characters.
//!
//! Game logic lives in this library so it is unit-testable; the binary in
//! `main.rs` only configures the window and adds [`GamePlugin`].

pub mod announcer;
pub mod arena;
pub mod audio;
pub mod character;
pub mod combat;
pub mod core;
pub mod creation;
pub mod items;
pub mod menu;
pub mod progression;
pub mod roster;
pub mod save;
pub mod settings;
pub mod shop;
pub mod theme;
pub mod ui_widgets;

use bevy::prelude::*;

/// Top-level plugin that wires up every feature plugin of the game.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(core::CorePlugin);
        app.add_plugins(character::CharacterPlugin);
        app.add_plugins(menu::MenuPlugin);
        app.add_plugins(creation::CreationPlugin);
        app.add_plugins(items::ItemsPlugin);
        app.add_plugins(arena::ArenaPlugin);
        app.add_plugins(combat::CombatPlugin);
        app.add_plugins(announcer::AnnouncerPlugin);
        app.add_plugins(audio::GameAudioPlugin);
        app.add_plugins(progression::ProgressionPlugin);
        app.add_plugins(roster::RosterPlugin);
        app.add_plugins(save::SavePlugin);
        app.add_plugins(settings::SettingsPlugin);
        app.add_plugins(shop::ShopPlugin);
    }
}
