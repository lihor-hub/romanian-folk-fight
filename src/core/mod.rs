//! Core plugin: global game states, camera, and screen-cleanup helpers.

mod projection;

use bevy::camera::Viewport;
use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::theme::is_mobile_width;

#[cfg(test)]
pub(crate) use projection::screen_point_for_world_point;
pub(crate) use projection::{letterbox_zoom, logical_node_rect, world_point_for_screen_point};

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
    /// Waits for [`UiFont`] and [`crate::theme::PanelTexture`] to finish
    /// loading before anything spawns UI. Fixes #114: on a cold wasm load
    /// the menu used to be built in [`GameState::MainMenu`] before the async
    /// asset fetches landed, so its first frame drew Bevy's fallback
    /// monospace font and untextured white panels — and neither one ever
    /// re-laid-out once the real assets arrived. See
    /// [`transition_out_of_loading`].
    #[default]
    Loading,
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

/// Gates [`GameState::Loading`] on the two handles every first-painted
/// screen depends on: [`UiFont`] and [`crate::theme::PanelTexture`]. Moves to
/// [`GameState::MainMenu`] only once `AssetServer::is_loaded_with_dependencies`
/// is true for both, so no screen ever spawns text or a panel border with an
/// unloaded handle (#114).
///
/// Headless test apps build without `AssetPlugin` (see [`load_ui_font`] and
/// `load_panel_texture`'s own `Option<Res<AssetServer>>` tolerance), so
/// nothing ever loads and this would hang forever waiting on it. When the
/// `AssetServer` resource is absent, fall straight through to `MainMenu`
/// instead.
fn transition_out_of_loading(
    ui_font: Res<UiFont>,
    panel_texture: Res<crate::theme::PanelTexture>,
    asset_server: Option<Res<AssetServer>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let Some(asset_server) = asset_server else {
        next_state.set(GameState::MainMenu);
        return;
    };
    let font_ready = asset_server.is_loaded_with_dependencies(ui_font.font.id());
    let panel_ready = asset_server.is_loaded_with_dependencies(panel_texture.image.id());
    if font_ready && panel_ready {
        next_state.set(GameState::MainMenu);
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

/// The centered 4:3 stage rect [`letterbox_camera`] fits the world camera's
/// viewport to, in **logical** UI pixels — the same unit convention as
/// [`ViewportInfo`] (#125). Every full-window UI overlay (the combat HUD, the
/// pause overlay, the combat-result dialogs) must size and position its root
/// node from this resource instead of `Val::Percent(100.0)`, or it bleeds
/// past the letterbox seams onto the bars.
///
/// `position` is the top-left corner of the rect and `size` its width/height,
/// both already converted from the physical pixels [`letterbox_camera`]
/// computes for the camera's [`Viewport`] — divided once by the window's
/// `scale_factor`, mirroring [`ViewportInfo::from_window`]'s single-conversion
/// rule so downstream UI code never has to reason about DPI itself.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LetterboxRect {
    pub position: Vec2,
    pub size: Vec2,
}

impl Default for LetterboxRect {
    /// The design resolution's own aspect ratio is exactly 4:3, so a rect at
    /// the origin sized to it is consistent with "no bars" — matching
    /// [`ViewportInfo::default`] before the real window size is known.
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            size: Vec2::new(LOGICAL_WIDTH, LOGICAL_HEIGHT),
        }
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<UiFont>()
            .init_resource::<ViewportInfo>()
            .init_resource::<LetterboxRect>()
            // The letterbox bars need a deliberate, uniform treatment (#125)
            // rather than Bevy's default clear color (a dark gray meant for
            // its own docs, not this game's palette): the world camera's
            // clear op covers the *entire* render target before its viewport
            // restricts where it actually draws, so pinning `ClearColor`
            // here is the simplest correct fix — no extra background node
            // needed on top of every screen.
            .insert_resource(ClearColor(crate::theme::NIGHT_BLACK))
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
            .add_systems(
                Update,
                transition_out_of_loading.run_if(in_state(GameState::Loading)),
            )
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
///
/// Also publishes that same rectangle as [`LetterboxRect`], in logical UI
/// pixels, so full-window UI overlays can constrain themselves to it (#125).
/// The rect is computed once in logical space (from [`ViewportInfo`]) and
/// only then scaled to physical pixels for the camera's [`Viewport`] — the
/// inverse of the usual physical-to-logical conversion direction, but
/// equivalent by linearity, and it means the logical rect never has to be
/// recovered by dividing an already-rounded physical value back down.
fn letterbox_camera(
    viewport: Res<ViewportInfo>,
    mut cameras: Query<&mut Camera, With<WorldCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut letterbox_rect: ResMut<LetterboxRect>,
) {
    if !viewport.is_changed() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let scale_factor = window.scale_factor();
    if viewport.width <= 0.0 || viewport.height <= 0.0 {
        return;
    }
    let target_aspect = LOGICAL_WIDTH / LOGICAL_HEIGHT;
    let window_aspect = viewport.width / viewport.height;
    let (rect_w, rect_h) = if window_aspect > target_aspect {
        (viewport.height * target_aspect, viewport.height)
    } else {
        (viewport.width, viewport.width / target_aspect)
    };
    let rect_position = Vec2::new(
        (viewport.width - rect_w) / 2.0,
        (viewport.height - rect_h) / 2.0,
    );
    let rect_size = Vec2::new(rect_w, rect_h);
    let next_rect = LetterboxRect {
        position: rect_position,
        size: rect_size,
    };
    if *letterbox_rect != next_rect {
        *letterbox_rect = next_rect;
    }

    let physical_position = UVec2::new(
        (rect_position.x * scale_factor).round() as u32,
        (rect_position.y * scale_factor).round() as u32,
    );
    let physical_size = UVec2::new(
        (rect_size.x * scale_factor).round() as u32,
        (rect_size.y * scale_factor).round() as u32,
    );
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
/// Despawns every entity tagged `T` (a screen's root marker), then clears
/// [`InputFocus`] (#216): every focusable entity is scoped to its own screen
/// (or an overlay above it), so whatever `InputFocus` named is either one of
/// the entities just despawned above or, at worst, about to be irrelevant
/// once the screen it belonged to is gone. Clearing here — once, centrally —
/// means a fresh screen always starts with focus unset (the existing #213
/// pointer-first default) rather than carrying a stale, possibly-despawned
/// `Entity` id into the next screen's `TabNavigation` queries. `Option` so
/// headless tests that never added [`crate::ui_widgets::focus::FocusNavigationPlugin`]
/// (and so never initialized this resource) keep working unchanged.
pub fn despawn_screen<T: Component>(
    mut commands: Commands,
    entities: Query<Entity, With<T>>,
    mut focus: Option<ResMut<InputFocus>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    if let Some(focus) = focus.as_mut() {
        focus.clear();
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
    fn initial_state_is_loading() {
        let mut app = test_app();
        app.update();
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(
            *state.get(),
            GameState::Loading,
            "the game must gate on the loading screen before the menu builds any UI (#114)"
        );
    }

    /// Guards the headless path (#114): apps built without `AssetPlugin` —
    /// every unit test in this repo — must still fall through to `MainMenu`
    /// instead of stalling in `Loading` forever, since nothing will ever
    /// report as loaded without a real `AssetServer`.
    #[test]
    fn headless_app_without_asset_plugin_reaches_main_menu() {
        let mut app = test_app();
        app.update(); // Loading entered; no `AssetServer` -> falls through immediately.
        app.update(); // the fall-through transition applies.
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::MainMenu);
    }

    /// With a real `AssetServer`, the gate must wait for *both* the font and
    /// the panel texture before ever entering `MainMenu` — one handle stuck
    /// (here, deliberately pointed at a path that will never resolve) must
    /// hold the whole gate, even while the other handle finishes loading.
    /// Once both handles are backed by real, loadable assets, the gate opens.
    #[test]
    fn loading_transitions_to_main_menu_only_once_both_handles_are_loaded() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::text::TextPlugin,
            bevy::image::ImagePlugin::default(),
            StatesPlugin,
            CorePlugin,
        ));
        // `ImagePlugin` only *reserves* the PNG loader's file extensions; the
        // loader itself is normally registered by `RenderPlugin`'s `finish()`
        // (which needs a render device we don't have here). Register a
        // headless-safe one directly so `panel_texture.image` can actually
        // finish loading in this test instead of stalling forever.
        app.register_asset_loader(bevy::image::ImageLoader::new(
            bevy::image::CompressedImageFormats::NONE,
        ));
        // PreStartup loads the real bundled font + panel texture.
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Loading,
            "must not race ahead of the asset fetches on the very first frame"
        );

        // Break just the font handle: a path that can never resolve keeps
        // `is_loaded_with_dependencies` false forever, so even once the
        // (still-genuine) panel texture finishes loading, the gate must
        // stay shut.
        {
            let asset_server = app.world().resource::<AssetServer>().clone();
            let mut ui_font = app.world_mut().resource_mut::<UiFont>();
            ui_font.font = asset_server.load("fonts/does-not-exist.ttf");
        }
        // On a fast machine both original (real) assets can finish loading
        // inside the very first `update()`, which queues the `MainMenu`
        // transition before the broken handle above is even installed. The
        // `State` assertion right after that update still passes — the
        // transition is only *pending* — so discard any queued transition
        // now; from here on the gate re-evaluates against the broken font.
        *app.world_mut().resource_mut::<NextState<GameState>>() = NextState::Unchanged;
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Loading,
            "one unresolved handle must block the transition even though the other loaded"
        );

        // Repair the font handle with the real asset; now both are genuine
        // loadable assets, so the gate must eventually open.
        {
            let asset_server = app.world().resource::<AssetServer>().clone();
            let mut ui_font = app.world_mut().resource_mut::<UiFont>();
            ui_font.font = asset_server.load(UI_FONT_PATH);
        }
        let mut reached_main_menu = false;
        for _ in 0..500 {
            app.update();
            if *app.world().resource::<State<GameState>>().get() == GameState::MainMenu {
                reached_main_menu = true;
                break;
            }
        }
        assert!(
            reached_main_menu,
            "loading must reach MainMenu once both handles report loaded"
        );
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
    /// `WorldCamera`'s computed physical `Viewport` alongside the published
    /// [`LetterboxRect`] after one update.
    fn run_letterbox_camera(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> (Viewport, LetterboxRect) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ViewportInfo::new(logical_width, logical_height));
        app.init_resource::<LetterboxRect>();
        app.world_mut().spawn((
            window_at(logical_width, logical_height, scale_factor),
            PrimaryWindow,
        ));
        let camera = app.world_mut().spawn((Camera::default(), WorldCamera)).id();
        app.add_systems(Update, letterbox_camera);
        app.update();
        let viewport = app
            .world()
            .get::<Camera>(camera)
            .and_then(|camera| camera.viewport.clone())
            .expect("letterbox_camera must set a physical viewport");
        let rect = *app.world().resource::<LetterboxRect>();
        (viewport, rect)
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
            let (viewport, _rect) = run_letterbox_camera(1600.0, 900.0, scale_factor);
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

    /// #125: the published [`LetterboxRect`] (logical px) must convert back
    /// to exactly the camera's physical `Viewport` at every aspect ratio the
    /// issue calls out — 16:10, exact 4:3, and a tall portrait window — swept
    /// across DPR 1/2/3, matching the existing scale-factor sweep above.
    #[test]
    fn letterbox_rect_converts_to_the_camera_viewport_across_aspects_and_scale_factors() {
        let cases: [(f32, f32, &str); 3] = [
            (1280.0, 800.0, "16:10 landscape"),
            (800.0, 600.0, "exact 4:3"),
            (375.0, 812.0, "tall portrait phone"),
        ];
        for (logical_width, logical_height, label) in cases {
            for scale_factor in [1.0, 2.0, 3.0] {
                let (viewport, rect) =
                    run_letterbox_camera(logical_width, logical_height, scale_factor);
                let expected_position = UVec2::new(
                    (rect.position.x * scale_factor).round() as u32,
                    (rect.position.y * scale_factor).round() as u32,
                );
                let expected_size = UVec2::new(
                    (rect.size.x * scale_factor).round() as u32,
                    (rect.size.y * scale_factor).round() as u32,
                );
                assert_eq!(
                    viewport.physical_position, expected_position,
                    "{label} at {scale_factor}x: rect position doesn't convert to the camera viewport"
                );
                assert_eq!(
                    viewport.physical_size, expected_size,
                    "{label} at {scale_factor}x: rect size doesn't convert to the camera viewport"
                );
            }
        }
    }

    /// #125 acceptance criterion: at exactly 4:3 there must be no bars — the
    /// rect spans the full logical window, at the origin.
    #[test]
    fn letterbox_rect_spans_the_full_window_at_exactly_4_3() {
        for scale_factor in [1.0, 2.0, 3.0] {
            let (_viewport, rect) = run_letterbox_camera(800.0, 600.0, scale_factor);
            assert_eq!(rect.position, Vec2::ZERO, "no bars means no offset");
            assert_eq!(
                rect.size,
                Vec2::new(800.0, 600.0),
                "rect spans the full 4:3 window"
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

    /// #216: a fresh screen must never inherit a stale, possibly-despawned
    /// focused `Entity` from the screen it replaced.
    #[test]
    fn despawn_screen_clears_stale_input_focus() {
        let mut app = App::new();
        app.init_resource::<InputFocus>();
        app.add_systems(Update, despawn_screen::<TestScreen>);
        let tagged = app.world_mut().spawn(TestScreen).id();
        app.world_mut()
            .insert_resource(InputFocus::from_entity(tagged));
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
    }

    /// A headless test app that never added
    /// [`crate::ui_widgets::focus::FocusNavigationPlugin`] (and so never
    /// initialized [`InputFocus`]) must not panic despawning a screen.
    #[test]
    fn despawn_screen_without_input_focus_resource_does_not_panic() {
        let mut app = App::new();
        app.add_systems(Update, despawn_screen::<TestScreen>);
        app.world_mut().spawn(TestScreen);
        app.update();
    }
}
