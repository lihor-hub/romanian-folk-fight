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
///
/// `width`/`height` are always **logical** (CSS-equivalent) pixels, never
/// physical/device pixels — see [`ViewportInfo::from_window`] (#115).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ViewportInfo {
    pub width: f32,
    pub height: f32,
    pub is_mobile: bool,
}

impl ViewportInfo {
    /// Builds from an already-logical width/height. Private and pure (no
    /// `Window` access) so the breakpoint math in [`is_mobile_width`] stays
    /// unit-testable without spinning up a window — callers that have a
    /// live `Window` should go through [`ViewportInfo::from_window`] instead
    /// of reaching for a physical-pixel field by mistake.
    fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            is_mobile: is_mobile_width(width),
        }
    }

    /// Builds from a live [`Window`], resolving to logical pixels regardless
    /// of `scale_factor` (#115). On a HiDPI display the window's *physical*
    /// pixel count is `scale_factor` times its logical size — e.g. a
    /// 1280-logical-px desktop window reports 2560 physical pixels at a 2x
    /// `scale_factor` — so the mobile breakpoint must compare against the
    /// logical width (1280), not that physical count, or a desktop-sized
    /// window would misread as a phone-sized one on any HiDPI display.
    /// [`Window::width`]/[`Window::height`] already divide physical size by
    /// `scale_factor` to produce this; routing every call site through this
    /// one helper keeps that the single, tested place that decision is made
    /// instead of leaving each caller to remember it independently.
    fn from_window(window: &Window) -> Self {
        Self::new(window.width(), window.height())
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
            // Bevy multiplies every `Val::Px` by `window.scale_factor() *
            // UiScale` exactly once to get a HiDPI-crisp physical pixel
            // size (#115). `UiScale`'s own default is already `1.0`, but
            // that reliance was implicit — pinned here explicitly so a
            // node declared 260px wide is guaranteed to paint 260 CSS px
            // (not 520) on any display, and so nothing can silently start
            // compounding a second multiplier onto `window.scale_factor()`.
            .insert_resource(UiScale(1.0))
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
        *viewport = ViewportInfo::from_window(window);
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
    let _ = windows; // resize events already carry the new (logical) size.
    // `WindowResized::width`/`height` are documented as logical pixels, the
    // same quantity `ViewportInfo::from_window` derives from a live
    // `Window` — kept as a direct `ViewportInfo::new` call (rather than
    // re-querying the window) since the event already carries the value.
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

    /// Builds a `Window` whose logical size is exactly `(logical_width,
    /// logical_height)` at the given `scale_factor`, by deriving the
    /// physical size that produces it (`physical = logical * scale_factor`)
    /// — mirroring what a real HiDPI display reports, e.g. a 1280-logical-px
    /// desktop window is 2560 physical pixels at a 2x `scale_factor`.
    fn window_at(logical_width: f32, logical_height: f32, scale_factor: f32) -> Window {
        let mut window = Window::default();
        window.resolution = bevy::window::WindowResolution::new(
            (logical_width * scale_factor).round() as u32,
            (logical_height * scale_factor).round() as u32,
        )
        .with_scale_factor_override(scale_factor);
        window
    }

    /// #115: on a HiDPI display the window's *physical* pixel count is
    /// `scale_factor` times its logical size. `ViewportInfo::from_window`
    /// must key the mobile breakpoint off the logical size, or a
    /// desktop-sized window at a 2x `scale_factor` would misread as
    /// phone-sized (1280 physical / 2 = 640 < the 700 breakpoint).
    #[test]
    fn viewport_info_from_window_uses_logical_width_regardless_of_scale_factor() {
        let desktop_hidpi = window_at(1280.0, 800.0, 2.0);
        let info = ViewportInfo::from_window(&desktop_hidpi);
        assert_eq!(info.width, 1280.0);
        assert!(
            !info.is_mobile,
            "a 1280-logical-px window must not be mobile at any scale factor"
        );

        let desktop_standard = window_at(1280.0, 800.0, 1.0);
        assert!(!ViewportInfo::from_window(&desktop_standard).is_mobile);

        let phone_hidpi = window_at(375.0, 812.0, 3.0);
        let info = ViewportInfo::from_window(&phone_hidpi);
        assert_eq!(info.width, 375.0);
        assert!(
            info.is_mobile,
            "a 375-logical-px window must be mobile at any scale factor"
        );

        let phone_2x = window_at(375.0, 812.0, 2.0);
        assert!(ViewportInfo::from_window(&phone_2x).is_mobile);
    }

    /// Builds a minimal app running only `letterbox_camera`, with a
    /// `PrimaryWindow` at the given `scale_factor` and a `ViewportInfo`
    /// fixed at `(logical_width, logical_height)`, and returns the
    /// `WorldCamera`'s computed physical `Viewport` after one update.
    fn run_letterbox_camera(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> Viewport {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ViewportInfo::new(logical_width, logical_height));
        app.world_mut().spawn((
            window_at(logical_width, logical_height, scale_factor),
            PrimaryWindow,
        ));
        let camera = app.world_mut().spawn((Camera::default(), WorldCamera)).id();
        app.add_systems(Update, letterbox_camera);
        app.update();
        app.world()
            .get::<Camera>(camera)
            .and_then(|camera| camera.viewport.clone())
            .expect("letterbox_camera must set a physical viewport")
    }

    /// #115: `letterbox_camera` must keep computing the same *physical*
    /// letterboxed rectangle (scaled proportionally by `scale_factor`) after
    /// the `ViewportInfo`/`UiScale` changes in this PR — it must not regress
    /// to using a `scale_factor`-independent (logical) size.
    #[test]
    fn letterbox_camera_scales_physical_viewport_with_scale_factor() {
        // A 1600x900 (16:9) logical window letterboxed to LOGICAL_WIDTH /
        // LOGICAL_HEIGHT's 4:3 aspect: full height, pillarboxed left/right.
        for (scale_factor, expected_position, expected_size) in [
            (1.0, UVec2::new(200, 0), UVec2::new(1200, 900)),
            (2.0, UVec2::new(400, 0), UVec2::new(2400, 1800)),
            (3.0, UVec2::new(600, 0), UVec2::new(3600, 2700)),
        ] {
            let viewport = run_letterbox_camera(1600.0, 900.0, scale_factor);
            assert_eq!(
                viewport.physical_position, expected_position,
                "wrong physical_position at scale_factor {scale_factor}"
            );
            assert_eq!(
                viewport.physical_size, expected_size,
                "wrong physical_size at scale_factor {scale_factor}"
            );
        }
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
