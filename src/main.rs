//! Romanian Folk Fight — binary entry point.
//!
//! This binary only configures the window and adds [`GamePlugin`]; game logic
//! lives in the library.

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use romanian_folk_fight::GamePlugin;
use romanian_folk_fight::core::{LOGICAL_HEIGHT, LOGICAL_WIDTH};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                // No .meta files ship with the assets; skipping the check
                // matters on wasm, where dev servers answer missing .meta
                // requests with the SPA fallback page and break the load.
                .set(AssetPlugin {
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Romanian Folk Fight".into(),
                        resolution: (LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32).into(),
                        canvas: Some("#game-canvas".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(GamePlugin)
        .run();
}
