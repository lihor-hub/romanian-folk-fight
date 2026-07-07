use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use romanian_folk_fight::GamePlugin;

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
                        resolution: (800, 600).into(),
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
