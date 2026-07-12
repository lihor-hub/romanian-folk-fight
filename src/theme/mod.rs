//! Single source of truth for UI styling (#28): the folk-textile palette,
//! spacing scale, `TextFont` presets, button style bundles, and the
//! embroidery-motif 9-slice panel texture. Every screen consumes this module
//! instead of defining its own color literals — `grep -rn "Color::srgb" src/
//! --include=*.rs` outside this module returns nothing (see `docs/art-direction.md`
//! for the palette source and motif rationale).

use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::NodeImageMode;

use crate::core::UiFont;

// --- Palette (docs/art-direction.md: Romanian folk textiles) ---

/// Deep red — primary accent, blood-red wool.
pub const DEEP_RED: Color = Color::srgb(0.55, 0.10, 0.10);
/// Cream — linen shirts, highlights, UI text.
pub const CREAM: Color = Color::srgb(0.96, 0.93, 0.84);
/// Black — outlines, hair, boots, night sky.
pub const NIGHT_BLACK: Color = Color::srgb(0.07, 0.06, 0.06);
/// Gold — trim, embroidery, boss accents.
pub const GOLD: Color = Color::srgb(0.788, 0.635, 0.153);
/// Warm arena backdrop — clay, torchlight, and carved wood instead of flat black.
pub const ARENA_BROWN: Color = Color::srgb(0.18, 0.11, 0.07);
/// Linen-washed panel surface used behind dense controls.
pub const PANEL_LINEN: Color = Color::srgba(0.22, 0.13, 0.08, 0.86);
/// Dark walnut strip for icon wells and body-slot rows.
pub const WALNUT: Color = Color::srgb(0.24, 0.13, 0.08);

/// Button background at rest.
pub const BUTTON_NORMAL: Color = Color::srgb(0.50, 0.09, 0.08);
/// Button background under the cursor.
pub const BUTTON_HOVERED: Color = Color::srgb(0.64, 0.15, 0.11);
/// Button background while pressed.
pub const BUTTON_PRESSED: Color = Color::srgb(0.36, 0.05, 0.04);
/// Greyed-out button background.
pub const BUTTON_DISABLED: Color = Color::srgb(0.25, 0.17, 0.13);
/// Text color on a disabled button.
pub const TEXT_DISABLED: Color = Color::srgb(0.76, 0.66, 0.61);

/// HP bar fill.
pub const HP_FILL: Color = Color::srgb(0.78, 0.16, 0.14);
/// Stamina / XP bar fill.
pub const STAMINA_FILL: Color = Color::srgb(0.88, 0.74, 0.22);
/// Carved-wood bar track behind any fill.
pub const BAR_TRACK: Color = Color::srgb(0.16, 0.14, 0.13);

/// Semi-transparent panel backdrop (HUD panels, combat log).
pub const PANEL_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
/// Darker modal scrim behind pause/settings overlays.
pub const SCRIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.65);
/// Heavier scrim for overlays stacked above another overlay (settings above pause).
pub const SCRIM_HEAVY: Color = Color::srgba(0.0, 0.0, 0.0, 0.75);
/// Opaque overlay panel background (pause overlay).
pub const OVERLAY_PANEL: Color = Color::srgb(0.12, 0.10, 0.09);

/// Muted footnote tone (victory-screen credits block).
pub const CREDITS_GRAY: Color = Color::srgb(0.55, 0.52, 0.48);
/// Arena ground strip.
pub const GROUND_COLOR: Color = Color::srgb(0.30, 0.22, 0.14);
/// Name-label color for boss opponents; regular fighters use [`CREAM`].
pub const BOSS_LABEL_COLOR: Color = Color::srgb(0.95, 0.45, 0.20);
/// Critical-hit FX color (shares the palette gold).
pub const CRIT_GOLD: Color = GOLD;
/// Blocked-hit FX color.
pub const BLOCKED_GRAY: Color = Color::srgb(0.62, 0.60, 0.58);
/// Combat-log / HUD banner backdrop.
pub const BANNER_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.72);

// --- Responsive breakpoint (#31: mobile touch layout) ---

/// Window width, in logical pixels, below which the UI reflows to the
/// mobile layout: bigger touch targets, a 2×2 action grid, a shorter combat
/// log. Matches the "portrait phone" cutoff called out in the issue.
pub const MOBILE_BREAKPOINT: f32 = 700.0;

/// Minimum touch-target size (both axes) for any interactive element, per
/// common mobile accessibility guidance.
pub const MIN_TOUCH_TARGET: f32 = 44.0;

/// Minimum size of a combat action button under the mobile breakpoint —
/// bigger than [`MIN_TOUCH_TARGET`] because these are the highest-frequency
/// taps in the game.
pub const ACTION_BUTTON_TOUCH_TARGET: f32 = 48.0;

/// Number of combat-log lines shown under the mobile breakpoint (the full
/// history still lives in `CombatLog`; this only trims what's displayed).
pub const MOBILE_LOG_LINES: usize = 3;

/// Whether a window of the given logical width should use the mobile
/// layout. Pure so the breakpoint choice is unit-testable without spinning
/// up a window.
pub fn is_mobile_width(width: f32) -> bool {
    width < MOBILE_BREAKPOINT
}

// --- Spacing scale ---

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;
pub const SPACE_XXL: f32 = 32.0;

// --- TextFont presets ---

/// Title-sized text (screen headlines).
pub const TITLE_SIZE: f32 = 56.0;
/// Heading-sized text (section titles, e.g. "Setări").
pub const HEADING_SIZE: f32 = 32.0;
/// Body-sized text (buttons, labels).
pub const BODY_SIZE: f32 = 24.0;

/// Bold title-preset [`TextFont`] using the bundled UI font.
pub fn title_font(ui_font: &UiFont) -> TextFont {
    ui_font.text_font_bold(TITLE_SIZE)
}

/// Bold heading-preset [`TextFont`].
pub fn heading_font(ui_font: &UiFont) -> TextFont {
    ui_font.text_font_bold(HEADING_SIZE)
}

/// Regular body-preset [`TextFont`].
pub fn body_font(ui_font: &UiFont) -> TextFont {
    ui_font.text_font(BODY_SIZE)
}

// --- Button style bundle ---

/// The four background colors a button cycles through; construct with
/// [`ButtonStyle::default_style`] and read with [`ButtonStyle::background_for`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonStyle {
    pub normal: Color,
    pub hovered: Color,
    pub pressed: Color,
    pub disabled: Color,
    pub text_normal: Color,
    pub text_disabled: Color,
}

impl ButtonStyle {
    /// The shared button styling used by every screen in the game.
    pub const fn default_style() -> Self {
        Self {
            normal: BUTTON_NORMAL,
            hovered: BUTTON_HOVERED,
            pressed: BUTTON_PRESSED,
            disabled: BUTTON_DISABLED,
            text_normal: CREAM,
            text_disabled: TEXT_DISABLED,
        }
    }

    /// The background for a given [`Interaction`], ignoring disabled state
    /// (callers gate disabled buttons separately, matching the existing
    /// per-screen `update_button_backgrounds` systems).
    pub fn background_for(&self, interaction: Interaction) -> Color {
        match interaction {
            Interaction::Pressed => self.pressed,
            Interaction::Hovered => self.hovered,
            Interaction::None => self.normal,
        }
    }
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self::default_style()
    }
}

// --- 9-slice embroidery panel border ---

/// Asset path of the self-generated embroidery-motif 9-slice panel texture
/// (`scripts/generate-ui-panel.py`; see `assets/CREDITS.md`).
pub const PANEL_BORDER_PATH: &str = "ui/panel_border.png";

/// Pixel inset of the panel texture's 9-slice border (matches `BORDER` in
/// `scripts/generate-ui-panel.py`). `pub(crate)` so other modules (e.g. the
/// combat HUD's desktop action-strip fit check) can reason about the
/// effective content inset a paneled node ends up with.
pub(crate) const PANEL_BORDER_INSET: f32 = 24.0;

/// Pixel size of the (square) panel-border texture — matches `SIZE` in
/// `scripts/generate-ui-panel.py`. Together with [`PANEL_BORDER_INSET`] it
/// pins down the sub-rects [`motif_band`] and [`motif_emblem`] sample, so a
/// re-authored texture only has to update these two constants.
pub(crate) const PANEL_TEXTURE_SIZE: f32 = 96.0;

/// The panel-border texture handle, loaded at startup by [`ThemePlugin`].
/// Defaults to `Handle::default()` so headless tests (no `AssetPlugin`) keep
/// working, matching [`UiFont`]'s pattern.
#[derive(Resource, Default)]
pub struct PanelTexture {
    pub image: Handle<Image>,
}

/// The 9-slice mode used everywhere the panel border is applied: corners stay
/// crisp at any panel size, the side embroidery tiles along each edge, and
/// the center stretches to fill it.
pub fn panel_slice_mode() -> NodeImageMode {
    NodeImageMode::Sliced(TextureSlicer {
        border: BorderRect::all(PANEL_BORDER_INSET),
        // The texture center is a flat translucent dark drawn over
        // `PANEL_LINEN`, so stretching it is correct.
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Tile { stretch_value: 1.0 },
        max_corner_scale: 1.0,
    })
}

/// An [`ImageNode`] rendering the embroidery-motif panel border, 9-sliced to
/// fit whatever `Node` size the caller gives it.
pub fn panel_border(panel_texture: &PanelTexture) -> ImageNode {
    ImageNode {
        image: panel_texture.image.clone(),
        image_mode: panel_slice_mode(),
        ..default()
    }
}

/// An [`ImageNode`] sampling the horizontal embroidery band from the
/// panel-border texture's top edge, between the two corner blocks: the
/// repeating gold cross-stitch (ii) diamonds on the deep-red band with its
/// gold trim lines. Tiles horizontally to whatever width the caller's `Node`
/// gives it, so it reads as a continuous embroidered strip (#121). Give the
/// node a height of [`PANEL_BORDER_INSET`] px to keep the stitches at their
/// authored 1:1 pixel scale.
pub fn motif_band(panel_texture: &PanelTexture) -> ImageNode {
    ImageNode {
        image: panel_texture.image.clone(),
        rect: Some(Rect::new(
            PANEL_BORDER_INSET,
            0.0,
            PANEL_TEXTURE_SIZE - PANEL_BORDER_INSET,
            PANEL_BORDER_INSET,
        )),
        image_mode: NodeImageMode::Tiled {
            tile_x: true,
            tile_y: false,
            stretch_value: 1.0,
        },
        ..default()
    }
}

/// An [`ImageNode`] cropping a single embroidered diamond emblem from the
/// panel-border texture's corner motif: the gold cross-stitch diamond with a
/// cream heart on its black corner block. Stretches to the caller's `Node`
/// size; keep that a multiple of [`PANEL_BORDER_INSET`] so the linear-sampled
/// upscale stays even (#121).
pub fn motif_emblem(panel_texture: &PanelTexture) -> ImageNode {
    ImageNode {
        image: panel_texture.image.clone(),
        rect: Some(Rect::new(0.0, 0.0, PANEL_BORDER_INSET, PANEL_BORDER_INSET)),
        image_mode: NodeImageMode::Stretch,
        ..default()
    }
}

/// Raises a single padding side to at least `PANEL_BORDER_INSET`.
///
/// Only `Val::Px` values are compared against the inset: a caller's pixel
/// padding smaller than the inset is raised to it, and one already at or
/// above the inset is preserved untouched (never shrunk). Non-`Px` values
/// (`Val::Percent`, `Val::Auto`, `Val::Vw`, ...) cannot be compared to a
/// pixel inset without layout context, so they pass through unchanged —
/// callers that give a paneled node non-`Px` padding remain responsible for
/// clearing the border themselves.
fn merge_inset_side(value: Val) -> Val {
    match value {
        Val::Px(px) if px < PANEL_BORDER_INSET => Val::Px(PANEL_BORDER_INSET),
        other => other,
    }
}

/// Merges caller-supplied padding with the minimum content inset every
/// paneled node needs to clear the 9-slice embroidered border, per side. See
/// [`merge_inset_side`] for the exact per-side rule (and its `Val` caveat).
pub fn merge_panel_padding(caller: UiRect) -> UiRect {
    UiRect {
        left: merge_inset_side(caller.left),
        right: merge_inset_side(caller.right),
        top: merge_inset_side(caller.top),
        bottom: merge_inset_side(caller.bottom),
    }
}

/// A `node` decorated with the embroidery 9-slice panel border — the shape
/// every menu panel, HUD fighter panel, shop row group, and result dialog
/// uses instead of a flat `BackgroundColor`.
///
/// The node's padding is merged with [`PANEL_BORDER_INSET`] (see
/// [`merge_panel_padding`]) so panel content — text, bars, readouts — always
/// clears the border art instead of being clipped by it (#120). Callers can
/// still ask for more breathing room than the inset by setting a larger
/// pixel padding; it will not be shrunk.
pub fn panel_bundle(panel_texture: &PanelTexture, mut node: Node) -> impl Bundle {
    node.padding = merge_panel_padding(node.padding);
    (node, panel_border(panel_texture))
}

fn load_panel_texture(
    mut panel_texture: ResMut<PanelTexture>,
    asset_server: Option<Res<AssetServer>>,
    images: Option<Res<Assets<Image>>>,
) {
    if let (Some(asset_server), Some(_images)) = (asset_server, images) {
        panel_texture.image = asset_server.load(PANEL_BORDER_PATH);
    }
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelTexture>()
            .add_systems(PreStartup, load_panel_texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_style_covers_every_interaction_state() {
        let style = ButtonStyle::default_style();
        assert_eq!(style.background_for(Interaction::None), BUTTON_NORMAL);
        assert_eq!(style.background_for(Interaction::Hovered), BUTTON_HOVERED);
        assert_eq!(style.background_for(Interaction::Pressed), BUTTON_PRESSED);
    }

    #[test]
    fn disabled_style_differs_from_normal() {
        let style = ButtonStyle::default_style();
        assert_ne!(style.disabled, style.normal);
        assert_ne!(style.text_disabled, style.text_normal);
    }

    /// WCAG 2.1 relative luminance formula for a color component.
    /// Used to compute contrast ratio between two colors.
    fn relative_luminance(color: Color) -> f32 {
        let [r, g, b, _] = color.to_linear().to_f32_array();
        let r = if r <= 0.03928 {
            r / 12.92
        } else {
            ((r + 0.055) / 1.055).powf(2.4)
        };
        let g = if g <= 0.03928 {
            g / 12.92
        } else {
            ((g + 0.055) / 1.055).powf(2.4)
        };
        let b = if b <= 0.03928 {
            b / 12.92
        } else {
            ((b + 0.055) / 1.055).powf(2.4)
        };
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Compute the contrast ratio between two colors using WCAG 2.1 formula.
    fn contrast_ratio(lighter: Color, darker: Color) -> f32 {
        let l_lighter = relative_luminance(lighter);
        let l_darker = relative_luminance(darker);
        let (l1, l2) = if l_lighter > l_darker {
            (l_lighter, l_darker)
        } else {
            (l_darker, l_lighter)
        };
        (l1 + 0.05) / (l2 + 0.05)
    }

    #[test]
    fn button_disabled_is_warm() {
        let disabled = BUTTON_DISABLED.to_linear().to_f32_array();
        let red = disabled[0];
        let blue = disabled[2];
        assert!(
            red >= blue,
            "BUTTON_DISABLED red channel ({}) should be >= blue channel ({})",
            red,
            blue
        );
    }

    #[test]
    fn text_disabled_has_sufficient_contrast_on_button_disabled() {
        let ratio = contrast_ratio(TEXT_DISABLED, BUTTON_DISABLED);
        assert!(
            ratio >= 3.0,
            "TEXT_DISABLED on BUTTON_DISABLED contrast ratio {} should be >= 3.0",
            ratio
        );
    }

    #[test]
    fn panel_slice_mode_uses_the_documented_inset() {
        let NodeImageMode::Sliced(slicer) = panel_slice_mode() else {
            panic!("panel_slice_mode must be Sliced");
        };
        assert_eq!(slicer.border, BorderRect::all(PANEL_BORDER_INSET));
        assert_eq!(
            slicer.sides_scale_mode,
            SliceScaleMode::Tile { stretch_value: 1.0 }
        );
        assert_eq!(slicer.max_corner_scale, 1.0);
    }

    /// #121: the divider band samples the top border strip between the two
    /// corner blocks, at its authored height, and tiles only horizontally.
    #[test]
    fn motif_band_samples_the_top_embroidery_strip_and_tiles_horizontally() {
        let band = motif_band(&PanelTexture::default());
        assert_eq!(
            band.rect,
            Some(Rect::new(
                PANEL_BORDER_INSET,
                0.0,
                PANEL_TEXTURE_SIZE - PANEL_BORDER_INSET,
                PANEL_BORDER_INSET
            ))
        );
        assert!(matches!(
            band.image_mode,
            NodeImageMode::Tiled {
                tile_x: true,
                tile_y: false,
                ..
            }
        ));
    }

    /// #121: the emblem crops exactly one corner block, which carries the
    /// full corner diamond motif.
    #[test]
    fn motif_emblem_crops_the_corner_motif() {
        let emblem = motif_emblem(&PanelTexture::default());
        assert_eq!(
            emblem.rect,
            Some(Rect::new(0.0, 0.0, PANEL_BORDER_INSET, PANEL_BORDER_INSET))
        );
        assert!(matches!(emblem.image_mode, NodeImageMode::Stretch));
    }

    #[test]
    fn theme_plugin_provides_a_panel_texture_resource() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ThemePlugin));
        app.update();
        assert!(app.world().get_resource::<PanelTexture>().is_some());
    }

    #[test]
    fn narrow_windows_use_the_mobile_layout() {
        assert!(is_mobile_width(375.0));
        assert!(is_mobile_width(414.0));
        assert!(!is_mobile_width(MOBILE_BREAKPOINT));
        assert!(!is_mobile_width(1280.0));
    }

    #[test]
    fn touch_targets_meet_the_documented_minimums() {
        const { assert!(MIN_TOUCH_TARGET >= 44.0) };
        const { assert!(ACTION_BUTTON_TOUCH_TARGET >= 48.0) };
    }

    // --- panel content inset (#120) ---

    /// A caller who sets no padding at all (the HUD's old bug) ends up with
    /// the full border inset on every side, not clipped by the frame.
    #[test]
    fn panel_bundle_pads_unset_node_to_the_border_inset() {
        let panel_texture = PanelTexture::default();
        let node = Node::default();
        assert_eq!(
            node.padding,
            UiRect::all(Val::Px(0.0)),
            "sanity: starts at 0"
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app
            .world_mut()
            .spawn(panel_bundle(&panel_texture, node))
            .id();
        let spawned = app.world().get::<Node>(entity).expect("Node component");
        for (side, val) in [
            ("left", spawned.padding.left),
            ("right", spawned.padding.right),
            ("top", spawned.padding.top),
            ("bottom", spawned.padding.bottom),
        ] {
            match val {
                Val::Px(px) => assert!(
                    px >= PANEL_BORDER_INSET,
                    "{side} padding {px} below the {PANEL_BORDER_INSET}px border inset"
                ),
                other => panic!("expected Val::Px padding on {side}, got {other:?}"),
            }
        }
    }

    /// A caller-supplied padding smaller than the inset (e.g. the HUD's old
    /// 10px) is raised to the inset, not left as-is.
    #[test]
    fn merge_panel_padding_raises_a_smaller_caller_value() {
        let merged = merge_panel_padding(UiRect::all(Val::Px(10.0)));
        assert_eq!(merged, UiRect::all(Val::Px(PANEL_BORDER_INSET)));
    }

    /// A caller-supplied padding larger than the inset (e.g. the menu's 28px
    /// panels) is preserved, never shrunk down to the inset.
    #[test]
    fn merge_panel_padding_preserves_a_larger_caller_value() {
        let larger = PANEL_BORDER_INSET + 6.0;
        let merged = merge_panel_padding(UiRect::all(Val::Px(larger)));
        assert_eq!(merged, UiRect::all(Val::Px(larger)));
    }

    /// A caller value exactly at the inset is left as-is (not bumped, not
    /// shrunk) — the floor is inclusive.
    #[test]
    fn merge_panel_padding_leaves_an_exact_match_untouched() {
        let merged = merge_panel_padding(UiRect::all(Val::Px(PANEL_BORDER_INSET)));
        assert_eq!(merged, UiRect::all(Val::Px(PANEL_BORDER_INSET)));
    }

    /// Each side is merged independently, so a caller can be under the inset
    /// on one edge and over it on another without cross-contamination.
    #[test]
    fn merge_panel_padding_merges_each_side_independently() {
        let caller = UiRect {
            left: Val::Px(4.0),
            right: Val::Px(40.0),
            top: Val::Px(0.0),
            bottom: Val::Px(PANEL_BORDER_INSET),
        };
        let merged = merge_panel_padding(caller);
        assert_eq!(merged.left, Val::Px(PANEL_BORDER_INSET), "raised to floor");
        assert_eq!(
            merged.right,
            Val::Px(40.0),
            "preserved, already above floor"
        );
        assert_eq!(merged.top, Val::Px(PANEL_BORDER_INSET), "raised to floor");
        assert_eq!(
            merged.bottom,
            Val::Px(PANEL_BORDER_INSET),
            "exact match left untouched"
        );
    }

    /// Non-`Px` padding (e.g. `Val::Percent`) cannot be compared to a pixel
    /// inset without layout context, so `merge_panel_padding` documents that
    /// it leaves those values untouched rather than guessing.
    #[test]
    fn merge_panel_padding_leaves_non_px_values_untouched() {
        let caller = UiRect::all(Val::Percent(5.0));
        let merged = merge_panel_padding(caller);
        assert_eq!(merged, caller, "non-Px padding passes through unchanged");

        let auto = UiRect::all(Val::Auto);
        assert_eq!(merge_panel_padding(auto), auto);
    }
}
