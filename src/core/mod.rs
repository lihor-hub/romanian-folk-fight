//! Core plugin: global game states, camera, and screen-cleanup helpers.

use bevy::prelude::*;

/// Top-level flow of the game; every screen scopes its systems and entities
/// to one of these states.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    MainMenu,
    CharacterCreation,
    Shop,
    Fight,
    FightResult,
    GameOver,
    Victory,
}

/// Asset path of the bundled UI font (Alegreya variable, OFL — see
/// `assets/CREDITS.md`). It covers the Romanian comma-below diacritics
/// Ș/ș/Ț/ț (U+0218–U+021B) plus Ă/ă, Â/â, Î/î that Bevy's default font lacks.
pub const UI_FONT_PATH: &str = "fonts/Alegreya-Variable.ttf";

/// The game-wide UI font. Every text spawn must build its [`TextFont`]
/// through [`UiFont::text_font`] / [`UiFont::text_font_bold`] instead of
/// constructing one directly, so no screen falls back to the default font.
///
/// Defaults to `Handle::default()` so headless tests (no `AssetPlugin`)
/// keep working; [`CorePlugin`] swaps in the real handle at startup.
#[derive(Resource, Default)]
pub struct UiFont {
    pub font: Handle<Font>,
}

impl UiFont {
    /// A [`TextFont`] of the given size using the bundled UI font.
    pub fn text_font(&self, font_size: f32) -> TextFont {
        TextFont {
            font: self.font.clone().into(),
            font_size: font_size.into(),
            ..default()
        }
    }

    /// Bold variant (weight 700 on the font's `wght` axis).
    pub fn text_font_bold(&self, font_size: f32) -> TextFont {
        TextFont {
            weight: FontWeight::BOLD,
            ..self.text_font(font_size)
        }
    }
}

/// Loads the bundled UI font into the [`UiFont`] resource. Tolerates a
/// missing [`AssetServer`] or an uninitialized `Font` asset type (headless
/// test apps without the text plugins) by keeping the default handle.
fn load_ui_font(
    mut ui_font: ResMut<UiFont>,
    asset_server: Option<Res<AssetServer>>,
    fonts: Option<Res<Assets<Font>>>,
) {
    if let (Some(asset_server), Some(_fonts)) = (asset_server, fonts) {
        ui_font.font = asset_server.load(UI_FONT_PATH);
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<UiFont>()
            .add_systems(PreStartup, load_ui_font)
            .add_systems(Startup, spawn_camera);
    }
}

/// Despawns every entity tagged with the screen marker `T`. Register it in
/// `OnExit(...)` so a screen cleans up after itself.
pub fn despawn_screen<T: Component>(mut commands: Commands, entities: Query<Entity, With<T>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app
    }

    #[test]
    fn initial_state_is_main_menu() {
        let mut app = test_app();
        app.update();
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::MainMenu);
    }

    #[test]
    fn next_state_transition_applies_on_update() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::Fight);
    }

    #[derive(Component)]
    struct TestScreen;

    /// The bundled font must actually map the Romanian diacritics — the
    /// comma-below letters (U+0218–U+021B) are the ones most fonts miss.
    #[test]
    fn bundled_font_covers_romanian_diacritics() {
        let data = include_bytes!("../../assets/fonts/Alegreya-Variable.ttf");
        let face = ttf_parser::Face::parse(data, 0).expect("font parses");
        for ch in ['Ș', 'ș', 'Ț', 'ț', 'Ă', 'ă', 'Â', 'â', 'Î', 'î'] {
            assert!(
                face.glyph_index(ch).is_some(),
                "font is missing glyph for {ch:?} (U+{:04X})",
                ch as u32
            );
        }
        // Regular + bold both live on the variable weight axis.
        let wght = face
            .variation_axes()
            .into_iter()
            .find(|a| a.tag == ttf_parser::Tag::from_bytes(b"wght"))
            .expect("font has a wght axis");
        assert!(wght.min_value <= 400.0 && wght.max_value >= 700.0);
    }

    #[test]
    fn core_plugin_provides_ui_font_and_helpers() {
        let mut app = test_app();
        app.update();
        let ui_font = app.world().resource::<UiFont>();
        let font = ui_font.text_font(24.0);
        assert_eq!(font.font_size, 24.0.into());
        assert_eq!(font.weight, FontWeight::default());
        let bold = ui_font.text_font_bold(24.0);
        assert_eq!(bold.weight, FontWeight::BOLD);
    }

    #[test]
    fn despawn_screen_removes_only_tagged_entities() {
        let mut app = App::new();
        app.add_systems(Update, despawn_screen::<TestScreen>);
        let tagged = app.world_mut().spawn(TestScreen).id();
        let untagged = app.world_mut().spawn_empty().id();
        app.update();
        assert!(app.world().get_entity(tagged).is_err());
        assert!(app.world().get_entity(untagged).is_ok());
    }
}
