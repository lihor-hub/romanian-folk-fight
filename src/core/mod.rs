//! Core plugin: global game states, camera, and screen-cleanup helpers.

use bevy::camera::Viewport;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::theme::is_mobile_width;

/// Fixed logical resolution the arena world is designed at (matches
/// `arena::ARENA_WIDTH`/`ARENA_HEIGHT`). The camera keeps this exact world
/// area on screen via [`letterbox_camera`], adding black bars instead of
/// stretching or cropping when the window's aspect ratio differs.
pub const LOGICAL_WIDTH: f32 = 800.0;
/// See [`LOGICAL_WIDTH`].
pub const LOGICAL_HEIGHT: f32 = 600.0;

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

/// Live window size plus the derived mobile/desktop layout choice (#31).
/// Every screen's spawn/update systems read this instead of querying the
/// window directly, so the breakpoint logic — [`is_mobile_width`] — stays in
/// one place.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ViewportInfo {
    pub width: f32,
    pub height: f32,
    pub is_mobile: bool,
}

impl ViewportInfo {
    fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            is_mobile: is_mobile_width(width),
        }
    }
}

impl Default for ViewportInfo {
    /// The desktop design resolution, so headless tests and the first frame
    /// (before the real window size is known) default to the non-mobile
    /// layout.
    fn default() -> Self {
        Self::new(LOGICAL_WIDTH, LOGICAL_HEIGHT)
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<UiFont>()
            .init_resource::<ViewportInfo>()
            // Registered here (idempotent) rather than assumed from
            // `WindowPlugin`, so headless test apps that skip the window
            // stack can still read `MessageReader<WindowResized>`.
            .add_message::<WindowResized>()
            .add_systems(PreStartup, load_ui_font)
            .add_systems(Startup, (spawn_camera, init_viewport_size))
            .add_systems(Update, (track_viewport_size, letterbox_camera).chain())
            // Every screen consumes the theme module's palette, spacing, and
            // panel texture, so it rides along with the other core resources
            // instead of every test app wiring it up separately.
            .add_plugins(crate::theme::ThemePlugin);
    }
}

/// Seeds [`ViewportInfo`] from the actual window size at startup — the
/// resource's `Default` only holds the design resolution, and no
/// [`WindowResized`] event fires until the window is later resized.
fn init_viewport_size(
    mut viewport: ResMut<ViewportInfo>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if let Ok(window) = windows.single() {
        *viewport = ViewportInfo::new(window.width(), window.height());
    }
}

/// Refreshes [`ViewportInfo`] from the primary window's logical size, but
/// only on an actual resize event — cheap on native/wasm where resizes are
/// rare, and it means downstream systems can `run_if(resource_changed)`.
fn track_viewport_size(
    mut resized: MessageReader<WindowResized>,
    mut viewport: ResMut<ViewportInfo>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(event) = resized.read().last() else {
        return;
    };
    let _ = windows; // resize events already carry the new size.
    let next = ViewportInfo::new(event.width, event.height);
    if next != *viewport {
        *viewport = next;
    }
}

/// Keeps the fixed [`LOGICAL_WIDTH`]x[`LOGICAL_HEIGHT`] arena fully visible
/// and undistorted at any window size: the projection stays `Fixed` to the
/// logical resolution, and the camera's pixel [`Viewport`] is shrunk to the
/// largest centered rectangle matching that aspect ratio, letterboxing the
/// remainder instead of stretching or cropping the scene.
fn letterbox_camera(
    viewport: Res<ViewportInfo>,
    mut cameras: Query<&mut Camera, With<WorldCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !viewport.is_changed() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let scale_factor = window.scale_factor();
    let physical_width = viewport.width * scale_factor;
    let physical_height = viewport.height * scale_factor;
    if physical_width <= 0.0 || physical_height <= 0.0 {
        return;
    }
    let target_aspect = LOGICAL_WIDTH / LOGICAL_HEIGHT;
    let window_aspect = physical_width / physical_height;
    let (viewport_w, viewport_h) = if window_aspect > target_aspect {
        (physical_height * target_aspect, physical_height)
    } else {
        (physical_width, physical_width / target_aspect)
    };
    let physical_position = UVec2::new(
        ((physical_width - viewport_w) / 2.0).round() as u32,
        ((physical_height - viewport_h) / 2.0).round() as u32,
    );
    let physical_size = UVec2::new(viewport_w.round() as u32, viewport_h.round() as u32);
    for mut camera in &mut cameras {
        camera.viewport = Some(Viewport {
            physical_position,
            physical_size,
            ..default()
        });
    }
}

/// Despawns every entity tagged with the screen marker `T`. Register it in
/// `OnExit(...)` so a screen cleans up after itself.
pub fn despawn_screen<T: Component>(mut commands: Commands, entities: Query<Entity, With<T>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

/// Marker for the world camera: the one [`letterbox_camera`] resizes to a
/// fixed-aspect sub-rectangle of the window. Distinguishes it from
/// [`UiCamera`] in every camera query below, since both are `Camera2d`.
#[derive(Component)]
pub struct WorldCamera;

/// Marker for the UI-only camera: full window, never letterboxed, so menu
/// and HUD layouts reflow to the whole viewport even while the arena world
/// is pillar/letterboxed to its fixed logical resolution (#31).
#[derive(Component)]
pub struct UiCamera;

/// Render layer used exclusively by the UI camera. World sprites stay on the
/// default layer (0) and are invisible to this camera, so it only ever
/// draws UI — the two cameras' outputs simply composite (world camera first,
/// UI camera on top with a transparent clear).
const UI_RENDER_LAYER: usize = 1;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        WorldCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed {
                width: LOGICAL_WIDTH,
                height: LOGICAL_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    // A second camera dedicated to UI: full window, always on top, so menus
    // and the HUD reflow across the whole viewport instead of being
    // letterboxed along with the fixed-resolution arena world (#31).
    commands.spawn((
        Camera2d,
        UiCamera,
        IsDefaultUiCamera,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(UI_RENDER_LAYER),
    ));
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
