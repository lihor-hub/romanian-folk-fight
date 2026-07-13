//! Named high-contrast theme tokens and automated contrast checks (#214,
//! parent #145).
//!
//! ## Why token *pairs*
//!
//! A single color can't be judged for accessibility in isolation — contrast
//! is a relationship between a foreground (usually text, or a bar fill) and
//! the surface it sits on. This module names every one of those
//! relationships the game currently renders as a [`TokenPair`]: a
//! `(foreground, background)` pair tagged with the WCAG 2.1 category it must
//! satisfy ([`ContrastKind::Text`] needs >=4.5:1, [`ContrastKind::NonText`]
//! needs >=3:1 — non-text applies to UI component boundaries like a bar fill
//! against its track, and to text that is part of an inactive/disabled
//! control, which WCAG 1.4.3 exempts from the full text ratio).
//!
//! ## Two palettes
//!
//! [`normal_token_pairs`] uses the *existing* [`crate::theme`] color
//! constants verbatim — normal-mode rendering is unchanged by this issue,
//! even where a pair (documented below) doesn't clear its own threshold.
//! [`high_contrast_token_pairs`] defines a second, stronger set of colors
//! for the same named pairs; [`accessibility_contrast`] asserts every
//! high-contrast pair clears its threshold and is never weaker than its
//! normal-palette counterpart.
//!
//! The runtime-switchable [`Palette`] resource resolves to one of these two
//! color sets from [`AccessibilityPreferences::high_contrast`] (see
//! [`sync_active_palette`]) so screens read colors through it instead of the
//! raw constants.
//!
//! ## Coverage (this slice)
//!
//! Wired at runtime: the combat HUD's HP/stamina bars and track, fighter
//! panel text, and the combat log ([`crate::combat::hud`]). Token *values*
//! for disabled button text and normal button text are defined and
//! contrast-tested here but not yet wired to every screen's buttons (menu,
//! shop, creation, pause, settings, and the combat action palette each keep
//! their own fixed-palette `update_button_backgrounds` system) — tracked as
//! a follow-up; see the #214 PR's "surfaces deferred" note.
//!
//! ## Prospective #150 contract
//!
//! No buff/debuff tokens exist yet. When #150 introduces status effects,
//! each meaningful effect must add its own named token pair here (following
//! this module's pattern) in addition to whatever color it uses — see
//! [`crate::arena::fx`]'s module docs for the full contract text covering
//! both the token and the cue side.

use bevy::prelude::*;

use crate::settings::AccessibilityPreferences;
use crate::theme::{
    BAR_TRACK, BOSS_LABEL_COLOR, BUTTON_DISABLED, BUTTON_NORMAL, CREAM, HP_FILL, PANEL_LINEN,
    STAMINA_FILL, TEXT_DISABLED,
};

// --- WCAG 2.1 contrast math (reused by theme::mod's existing disabled-text
// test, and by every [`TokenPair`] below) ---------------------------------

/// WCAG 2.1 relative luminance of one color.
///
/// The WCAG formula linearizes *gamma-encoded sRGB* components, so this
/// starts from [`Color::to_srgba`] — not `to_linear()`, which would apply
/// the gamma expansion a second time and understate every luminance (the
/// in-test helper this replaced had exactly that bug; ratios here are the
/// true WCAG values).
pub(crate) fn relative_luminance(color: Color) -> f32 {
    let srgba = color.to_srgba();
    fn channel(c: f32) -> f32 {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(srgba.red) + 0.7152 * channel(srgba.green) + 0.0722 * channel(srgba.blue)
}

/// WCAG 2.1 contrast ratio between two colors (order-independent, always
/// `>= 1.0`).
pub(crate) fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (lighter, darker) = if la > lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

// --- Token pairs -----------------------------------------------------------

/// Which WCAG 2.1 contrast minimum a [`TokenPair`] must clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastKind {
    /// Ordinary readable text: WCAG 1.4.3, >= 4.5:1.
    Text,
    /// UI component / graphical object boundaries (e.g. a bar fill against
    /// its track), and text that is part of an inactive control (WCAG 1.4.3
    /// exempts these from the full text ratio): WCAG 1.4.11, >= 3:1.
    NonText,
}

impl ContrastKind {
    /// The minimum passing ratio for this category.
    pub const fn threshold(self) -> f32 {
        match self {
            Self::Text => 4.5,
            Self::NonText => 3.0,
        }
    }
}

/// One named, currently-rendered foreground-on-background relationship.
#[derive(Debug, Clone, Copy)]
pub struct TokenPair {
    /// Stable identifier, e.g. `"HP_BAR_ON_TRACK"`. Matches between the
    /// normal and high-contrast sets so [`accessibility_contrast`] can pair
    /// them up.
    pub name: &'static str,
    pub foreground: Color,
    pub background: Color,
    pub kind: ContrastKind,
}

impl TokenPair {
    /// This pair's actual WCAG contrast ratio.
    pub fn ratio(&self) -> f32 {
        contrast_ratio(self.foreground, self.background)
    }

    /// Whether the pair clears its [`ContrastKind::threshold`].
    pub fn passes(&self) -> bool {
        self.ratio() >= self.kind.threshold()
    }
}

// --- Normal palette (existing constants, unchanged rendering) -------------

const NORMAL_TEXT_PRIMARY: Color = CREAM;
const NORMAL_BUTTON_TEXT: Color = CREAM;
const NORMAL_TEXT_DISABLED: Color = TEXT_DISABLED;
const NORMAL_BUTTON_NORMAL: Color = BUTTON_NORMAL;
const NORMAL_BUTTON_DISABLED: Color = BUTTON_DISABLED;
const NORMAL_HP_FILL: Color = HP_FILL;
const NORMAL_STAMINA_FILL: Color = STAMINA_FILL;
const NORMAL_BAR_TRACK: Color = BAR_TRACK;
const NORMAL_BOSS_LABEL: Color = BOSS_LABEL_COLOR;
const NORMAL_PANEL_SURFACE: Color = PANEL_LINEN;

// --- High-contrast palette (stronger variants for the same named pairs) ---
//
// Panel *background* art (the 9-slice embroidery texture, approximated here
// by PANEL_LINEN) isn't re-themed by this slice, so panel-surface pairs get
// their strength entirely from a stronger foreground. Bar/button pairs get
// stronger colors on both sides where that's what clears the threshold.

const HC_TEXT_PRIMARY: Color = Color::srgb(1.0, 1.0, 1.0);
const HC_BUTTON_TEXT: Color = Color::srgb(1.0, 1.0, 1.0);
const HC_TEXT_DISABLED: Color = Color::srgb(0.92, 0.90, 0.88);
const HC_BUTTON_NORMAL: Color = Color::srgb(0.45, 0.03, 0.03);
const HC_BUTTON_DISABLED: Color = Color::srgb(0.16, 0.10, 0.08);
const HC_HP_FILL: Color = Color::srgb(0.95, 0.20, 0.16);
const HC_STAMINA_FILL: Color = Color::srgb(0.95, 0.80, 0.25);
const HC_BAR_TRACK: Color = Color::srgb(0.05, 0.04, 0.04);
const HC_BOSS_LABEL: Color = Color::srgb(1.0, 0.55, 0.25);
const HC_PANEL_SURFACE: Color = PANEL_LINEN;

/// Named token pair identifiers, documented once here and reused as the
/// literal `name` strings on both palettes' [`TokenPair`]s below.
pub const TEXT_PRIMARY_ON_PANEL: &str = "TEXT_PRIMARY_ON_PANEL";
pub const COMBAT_LOG_TEXT_ON_PANEL: &str = "COMBAT_LOG_TEXT_ON_PANEL";
pub const BUTTON_TEXT_ON_BUTTON: &str = "BUTTON_TEXT_ON_BUTTON";
pub const TEXT_DISABLED_ON_BUTTON: &str = "TEXT_DISABLED_ON_BUTTON";
pub const HP_BAR_ON_TRACK: &str = "HP_BAR_ON_TRACK";
pub const STAMINA_BAR_ON_TRACK: &str = "STAMINA_BAR_ON_TRACK";
pub const BOSS_LABEL_ON_PANEL: &str = "BOSS_LABEL_ON_PANEL";

/// Every documented token pair, rendered with the normal palette's colors.
/// [`normal_token_pairs`] and [`high_contrast_token_pairs`] must always
/// return the same `name`s in the same order (asserted by
/// [`accessibility_contrast`]).
pub fn normal_token_pairs() -> Vec<TokenPair> {
    vec![
        TokenPair {
            name: TEXT_PRIMARY_ON_PANEL,
            foreground: NORMAL_TEXT_PRIMARY,
            background: NORMAL_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: COMBAT_LOG_TEXT_ON_PANEL,
            foreground: NORMAL_TEXT_PRIMARY,
            background: NORMAL_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: BUTTON_TEXT_ON_BUTTON,
            foreground: NORMAL_BUTTON_TEXT,
            background: NORMAL_BUTTON_NORMAL,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: TEXT_DISABLED_ON_BUTTON,
            foreground: NORMAL_TEXT_DISABLED,
            background: NORMAL_BUTTON_DISABLED,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: HP_BAR_ON_TRACK,
            foreground: NORMAL_HP_FILL,
            background: NORMAL_BAR_TRACK,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: STAMINA_BAR_ON_TRACK,
            foreground: NORMAL_STAMINA_FILL,
            background: NORMAL_BAR_TRACK,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: BOSS_LABEL_ON_PANEL,
            foreground: NORMAL_BOSS_LABEL,
            background: NORMAL_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
    ]
}

/// The same named token pairs as [`normal_token_pairs`], with the
/// high-contrast palette's stronger colors.
pub fn high_contrast_token_pairs() -> Vec<TokenPair> {
    vec![
        TokenPair {
            name: TEXT_PRIMARY_ON_PANEL,
            foreground: HC_TEXT_PRIMARY,
            background: HC_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: COMBAT_LOG_TEXT_ON_PANEL,
            foreground: HC_TEXT_PRIMARY,
            background: HC_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: BUTTON_TEXT_ON_BUTTON,
            foreground: HC_BUTTON_TEXT,
            background: HC_BUTTON_NORMAL,
            kind: ContrastKind::Text,
        },
        TokenPair {
            name: TEXT_DISABLED_ON_BUTTON,
            foreground: HC_TEXT_DISABLED,
            background: HC_BUTTON_DISABLED,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: HP_BAR_ON_TRACK,
            foreground: HC_HP_FILL,
            background: HC_BAR_TRACK,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: STAMINA_BAR_ON_TRACK,
            foreground: HC_STAMINA_FILL,
            background: HC_BAR_TRACK,
            kind: ContrastKind::NonText,
        },
        TokenPair {
            name: BOSS_LABEL_ON_PANEL,
            foreground: HC_BOSS_LABEL,
            background: HC_PANEL_SURFACE,
            kind: ContrastKind::Text,
        },
    ]
}

// --- Runtime-switchable palette --------------------------------------------

/// The colors screens resolve through instead of the raw [`crate::theme`]
/// constants, for the surfaces wired in this slice (see this module's docs
/// for the coverage list). Switched by [`sync_active_palette`] from
/// [`AccessibilityPreferences::high_contrast`].
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub text_primary: Color,
    pub combat_log_text: Color,
    pub button_text: Color,
    pub button_normal: Color,
    pub text_disabled: Color,
    pub button_disabled: Color,
    pub hp_fill: Color,
    pub stamina_fill: Color,
    pub bar_track: Color,
    pub boss_label: Color,
}

impl Palette {
    /// The normal-mode palette (existing colors, unchanged rendering).
    pub const fn normal() -> Self {
        Self {
            text_primary: NORMAL_TEXT_PRIMARY,
            combat_log_text: NORMAL_TEXT_PRIMARY,
            button_text: NORMAL_BUTTON_TEXT,
            button_normal: NORMAL_BUTTON_NORMAL,
            text_disabled: NORMAL_TEXT_DISABLED,
            button_disabled: NORMAL_BUTTON_DISABLED,
            hp_fill: NORMAL_HP_FILL,
            stamina_fill: NORMAL_STAMINA_FILL,
            bar_track: NORMAL_BAR_TRACK,
            boss_label: NORMAL_BOSS_LABEL,
        }
    }

    /// The high-contrast palette (stronger variants meeting the documented
    /// WCAG targets; see [`accessibility_contrast`]).
    pub const fn high_contrast() -> Self {
        Self {
            text_primary: HC_TEXT_PRIMARY,
            combat_log_text: HC_TEXT_PRIMARY,
            button_text: HC_BUTTON_TEXT,
            button_normal: HC_BUTTON_NORMAL,
            text_disabled: HC_TEXT_DISABLED,
            button_disabled: HC_BUTTON_DISABLED,
            hp_fill: HC_HP_FILL,
            stamina_fill: HC_STAMINA_FILL,
            bar_track: HC_BAR_TRACK,
            boss_label: HC_BOSS_LABEL,
        }
    }

    /// [`Self::high_contrast`] if the preference is on, else [`Self::normal`].
    pub fn for_preferences(preferences: &AccessibilityPreferences) -> Self {
        if preferences.high_contrast {
            Self::high_contrast()
        } else {
            Self::normal()
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::normal()
    }
}

/// Keeps the [`Palette`] resource in sync with
/// [`AccessibilityPreferences::high_contrast`], so screens that read
/// `Res<Palette>` (or a system gated on `resource_changed::<Palette>`, like
/// [`crate::combat::hud::sync_hud_palette`]) switch at runtime without a
/// reload. A no-op frame when the preference hasn't changed, or resolves to
/// the palette already active (so `Palette` change detection only fires on
/// an actual switch).
pub fn sync_active_palette(
    accessibility: Res<AccessibilityPreferences>,
    mut palette: ResMut<Palette>,
) {
    if !accessibility.is_changed() {
        return;
    }
    let resolved = Palette::for_preferences(&accessibility);
    if *palette != resolved {
        *palette = resolved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red-first (#214): every documented token pair, in both palettes,
    /// clears its WCAG threshold in high-contrast mode, and is never weaker
    /// than the normal palette's same pair. Also doubles as a
    /// machine-readable contrast report (run with `--nocapture` to see the
    /// table).
    #[test]
    fn accessibility_contrast() {
        let normal = normal_token_pairs();
        let high_contrast = high_contrast_token_pairs();
        assert_eq!(
            normal.len(),
            high_contrast.len(),
            "normal and high-contrast must document the same set of token pairs"
        );

        let mut report = String::from(
            "token,kind,threshold,normal_ratio,normal_pass,high_contrast_ratio,high_contrast_pass\n",
        );
        let mut failures = Vec::new();
        for (n, h) in normal.iter().zip(high_contrast.iter()) {
            assert_eq!(
                n.name, h.name,
                "normal_token_pairs/high_contrast_token_pairs order must match"
            );
            assert_eq!(
                n.kind, h.kind,
                "{}: kind must match across palettes",
                n.name
            );

            let (n_ratio, h_ratio) = (n.ratio(), h.ratio());
            report.push_str(&format!(
                "{},{:?},{:.1},{:.2},{},{:.2},{}\n",
                n.name,
                n.kind,
                n.kind.threshold(),
                n_ratio,
                n.passes(),
                h_ratio,
                h.passes()
            ));

            if !h.passes() {
                failures.push(format!(
                    "{}: high-contrast ratio {:.2} is below its {:?} threshold {:.1}",
                    h.name,
                    h_ratio,
                    h.kind,
                    h.kind.threshold()
                ));
            }
            if h_ratio + 1e-4 < n_ratio {
                failures.push(format!(
                    "{}: high-contrast ratio {:.2} is weaker than the normal palette's {:.2}",
                    h.name, h_ratio, n_ratio
                ));
            }
        }

        println!("accessibility contrast report:\n{report}");
        assert!(
            failures.is_empty(),
            "accessibility contrast failures:\n{}\n\nfull report:\n{report}",
            failures.join("\n")
        );
    }

    #[test]
    fn normal_palette_matches_the_existing_theme_constants_unchanged() {
        let palette = Palette::normal();
        assert_eq!(palette.text_primary, CREAM);
        assert_eq!(palette.hp_fill, HP_FILL);
        assert_eq!(palette.stamina_fill, STAMINA_FILL);
        assert_eq!(palette.bar_track, BAR_TRACK);
        assert_eq!(palette.button_normal, BUTTON_NORMAL);
        assert_eq!(palette.text_disabled, TEXT_DISABLED);
        assert_eq!(palette.button_disabled, BUTTON_DISABLED);
        assert_eq!(palette.boss_label, BOSS_LABEL_COLOR);
    }

    #[test]
    fn for_preferences_selects_the_palette_from_the_high_contrast_flag() {
        let off = AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: false,
        };
        let on = AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: true,
        };
        assert_eq!(Palette::for_preferences(&off), Palette::normal());
        assert_eq!(Palette::for_preferences(&on), Palette::high_contrast());
    }

    #[test]
    fn sync_active_palette_switches_the_resource_when_the_preference_flips() {
        let mut app = App::new();
        app.init_resource::<AccessibilityPreferences>();
        app.init_resource::<Palette>();
        app.add_systems(Update, sync_active_palette);

        app.update();
        assert_eq!(*app.world().resource::<Palette>(), Palette::normal());

        app.insert_resource(AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: true,
        });
        app.update();
        assert_eq!(*app.world().resource::<Palette>(), Palette::high_contrast());

        app.insert_resource(AccessibilityPreferences {
            reduced_motion: false,
            high_contrast: false,
        });
        app.update();
        assert_eq!(*app.world().resource::<Palette>(), Palette::normal());
    }

    #[test]
    fn every_pair_kind_matches_the_documented_wcag_semantics() {
        for pair in normal_token_pairs() {
            let expected = match pair.name {
                TEXT_DISABLED_ON_BUTTON | HP_BAR_ON_TRACK | STAMINA_BAR_ON_TRACK => {
                    ContrastKind::NonText
                }
                _ => ContrastKind::Text,
            };
            assert_eq!(pair.kind, expected, "{}", pair.name);
        }
    }
}
