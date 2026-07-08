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

/// Button background at rest.
pub const BUTTON_NORMAL: Color = DEEP_RED;
/// Button background under the cursor.
pub const BUTTON_HOVERED: Color = Color::srgb(0.68, 0.16, 0.14);
/// Button background while pressed.
pub const BUTTON_PRESSED: Color = Color::srgb(0.42, 0.06, 0.06);
/// Greyed-out button background.
pub const BUTTON_DISABLED: Color = Color::srgb(0.35, 0.33, 0.31);
/// Text color on a disabled button.
pub const TEXT_DISABLED: Color = Color::srgb(0.60, 0.58, 0.55);

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
/// `scripts/generate-ui-panel.py`).
const PANEL_BORDER_INSET: f32 = 24.0;

/// The panel-border texture handle, loaded at startup by [`ThemePlugin`].
/// Defaults to `Handle::default()` so headless tests (no `AssetPlugin`) keep
/// working, matching [`UiFont`]'s pattern.
#[derive(Resource, Default)]
pub struct PanelTexture {
    pub image: Handle<Image>,
}

/// The 9-slice mode used everywhere the panel border is applied: corners stay
/// crisp at any panel size, sides and center stretch to fill it.
pub fn panel_slice_mode() -> NodeImageMode {
    NodeImageMode::Sliced(TextureSlicer {
        border: BorderRect::all(PANEL_BORDER_INSET),
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
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

/// A `node` decorated with the embroidery 9-slice panel border — the shape
/// every menu panel, HUD fighter panel, shop row group, and result dialog
/// uses instead of a flat `BackgroundColor`.
pub fn panel_bundle(panel_texture: &PanelTexture, node: Node) -> impl Bundle {
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

    #[test]
    fn panel_slice_mode_uses_the_documented_inset() {
        let NodeImageMode::Sliced(slicer) = panel_slice_mode() else {
            panic!("panel_slice_mode must be Sliced");
        };
        assert_eq!(slicer.border, BorderRect::all(PANEL_BORDER_INSET));
        assert_eq!(slicer.max_corner_scale, 1.0);
    }

    #[test]
    fn theme_plugin_provides_a_panel_texture_resource() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ThemePlugin));
        app.update();
        assert!(app.world().get_resource::<PanelTexture>().is_some());
    }
}
