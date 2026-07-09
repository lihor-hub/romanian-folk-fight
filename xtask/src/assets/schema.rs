//! Per-directory manifest sidecar schema (v1).
//!
//! A sidecar is a small `manifest.toml` file placed *inside* the directory
//! it describes. It lists the files directly in that directory only -- the
//! `path` field on every record/ignore entry is a bare file name (no `/`),
//! which structurally enforces the ownership boundary from #167: a sidecar
//! can never reach into a sibling directory, so adding a new asset directory
//! never requires editing an unrelated sidecar.
//!
//! # Fields
//!
//! Every record carries: a globally unique dotted `id`, its `path` (bare
//! file name), `kind` (media family), `category` (fine-grained role),
//! `status` (`runtime` | `source` | `legacy`), `provenance`, and an explicit
//! `license`. Everything else is optional and required only where it
//! applies:
//!
//! - `dimensions`: required for raster `image` records (not `.svg`, which is
//!   resolution-independent); always absent for `audio`/`font`/`document`.
//! - `sampler`: required for `image` records whose `status` is `runtime`
//!   (only a runtime-sampled image has an active filter mode); absent
//!   otherwise.
//! - `attachment`/`pivot`/`display`: required for `runtime`-status fighter
//!   and gear body-part/overlay categories (`fighter-runtime-part`,
//!   `gear-runtime-part`, `gear-overlay`) -- the rig attachment metadata
//!   called out in the issue. Absent for every other category.
//! - `crop`: always optional. The source-sheet crop rectangle for the
//!   fighter/gear runtime parts was never recorded anywhere in the repo
//!   (the sheets were hand-cropped in an image editor), so honest sidecars
//!   record the literal string `"unknown"` here rather than invent
//!   coordinates. See `xtask/README.md` for this known limitation.
//!
//! # Ignoring a file
//!
//! A file that is not itself an asset record (a directory `README.md`, or
//! the aggregate `assets/CREDITS.md`) is listed under `[[ignore]]` with a
//! human-readable `reason`. Every file under `assets/` must end up in either
//! a `record` or an `ignore` entry in exactly one sidecar (see
//! `aggregate.rs`) -- the one hardcoded exemption is `manifest.toml` itself,
//! which is sidecar infrastructure, not an asset.

use std::fmt;

use serde::Deserialize;

/// The only schema version this build understands. Bumped whenever a
/// breaking change is made to the fields below.
pub const SCHEMA_VERSION: u32 = 1;

/// One parsed `manifest.toml` sidecar, before path resolution.
#[derive(Debug, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    #[serde(default, rename = "ignore")]
    pub ignores: Vec<IgnoreEntry>,
    #[serde(default, rename = "record")]
    pub records: Vec<Record>,
}

/// A file in the sidecar's directory that is deliberately not an asset
/// record.
#[derive(Debug, Deserialize, Clone)]
pub struct IgnoreEntry {
    pub path: String,
    pub reason: String,
}

/// One asset record.
#[derive(Debug, Deserialize, Clone)]
pub struct Record {
    pub id: String,
    pub path: String,
    pub kind: Kind,
    pub category: Category,
    pub status: Status,
    pub provenance: String,
    pub license: String,

    /// Generator script, for `provenance = "repo-generated"` records.
    #[serde(default)]
    pub generator: Option<String>,
    /// Id of the source-sheet record this runtime part was cropped from.
    #[serde(default)]
    pub source_sheet: Option<String>,
    /// Path to a bundled license text file, for fonts.
    #[serde(default)]
    pub license_file: Option<String>,

    #[serde(default)]
    pub dimensions: Option<[u32; 2]>,
    #[serde(default)]
    pub sampler: Option<Sampler>,

    #[serde(default)]
    pub attachment: Option<String>,
    #[serde(default)]
    pub pivot: Option<[f32; 2]>,
    #[serde(default)]
    pub display: Option<[f32; 2]>,
    /// Source-sheet crop rectangle as `"x,y,w,h"`, or the literal string
    /// `"unknown"` when genuinely not tracked (see module docs).
    #[serde(default)]
    pub crop: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Image,
    Audio,
    Font,
    Document,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Kind::Image => "image",
            Kind::Audio => "audio",
            Kind::Font => "font",
            Kind::Document => "document",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Sprite,
    Background,
    UiIcon,
    UiPanel,
    UiSourceSheet,
    FighterSourceSheet,
    FighterRuntimePart,
    GearSourceSheet,
    GearRuntimePart,
    GearOverlay,
    Font,
    FontLicense,
    WebIcon,
    WebImage,
    Music,
    Sfx,
    Sting,
}

impl Category {
    /// The `Kind` every record of this category must declare. Backs the
    /// "wrongly classified media" diagnostic.
    pub fn expected_kind(self) -> Kind {
        match self {
            Category::Sprite
            | Category::Background
            | Category::UiIcon
            | Category::UiPanel
            | Category::UiSourceSheet
            | Category::FighterSourceSheet
            | Category::FighterRuntimePart
            | Category::GearSourceSheet
            | Category::GearRuntimePart
            | Category::GearOverlay
            | Category::WebIcon
            | Category::WebImage => Kind::Image,
            Category::Font => Kind::Font,
            Category::FontLicense => Kind::Document,
            Category::Music | Category::Sfx | Category::Sting => Kind::Audio,
        }
    }

    /// Whether this category is a fighter/gear rig attachment that must
    /// carry `attachment`/`pivot`/`display` metadata when `status = runtime`.
    pub fn is_rig_attachment(self) -> bool {
        matches!(
            self,
            Category::FighterRuntimePart | Category::GearRuntimePart | Category::GearOverlay
        )
    }

    /// Whether this category is served straight to the browser (via
    /// `index.html`) rather than decoded through Bevy's `AssetServer`. Web
    /// assets never have an active Bevy image sampler, regardless of
    /// `status`.
    pub fn is_web(self) -> bool {
        matches!(self, Category::WebIcon | Category::WebImage)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Sprite => "sprite",
            Category::Background => "background",
            Category::UiIcon => "ui-icon",
            Category::UiPanel => "ui-panel",
            Category::UiSourceSheet => "ui-source-sheet",
            Category::FighterSourceSheet => "fighter-source-sheet",
            Category::FighterRuntimePart => "fighter-runtime-part",
            Category::GearSourceSheet => "gear-source-sheet",
            Category::GearRuntimePart => "gear-runtime-part",
            Category::GearOverlay => "gear-overlay",
            Category::Font => "font",
            Category::FontLicense => "font-license",
            Category::WebIcon => "web-icon",
            Category::WebImage => "web-image",
            Category::Music => "music",
            Category::Sfx => "sfx",
            Category::Sting => "sting",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Runtime,
    Source,
    Legacy,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Status::Runtime => "runtime",
            Status::Source => "source",
            Status::Legacy => "legacy",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sampler {
    Nearest,
    Linear,
}

impl fmt::Display for Sampler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Sampler::Nearest => "nearest",
            Sampler::Linear => "linear",
        };
        write!(f, "{s}")
    }
}

/// The file extensions expected for each `Kind`, used by the "wrongly
/// classified" check. `svg` is intentionally included under `Image`
/// alongside raster formats.
pub fn expected_extensions(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Image => &["png", "svg"],
        Kind::Audio => &["ogg"],
        Kind::Font => &["ttf"],
        Kind::Document => &["txt", "md"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_sidecar() {
        let toml = r#"
            version = 1

            [[ignore]]
            path = "README.md"
            reason = "directory documentation, not an asset"

            [[record]]
            id = "sprites.player"
            path = "player.png"
            kind = "image"
            category = "sprite"
            status = "runtime"
            provenance = "repo-generated"
            license = "CC0 1.0"
            dimensions = [512, 512]
            sampler = "linear"
        "#;
        let sidecar: Sidecar = toml::from_str(toml).expect("valid sidecar");
        assert_eq!(sidecar.version, SCHEMA_VERSION);
        assert_eq!(sidecar.ignores.len(), 1);
        assert_eq!(sidecar.records.len(), 1);
        assert_eq!(sidecar.records[0].id, "sprites.player");
        assert_eq!(sidecar.records[0].dimensions, Some([512, 512]));
        assert_eq!(sidecar.records[0].sampler, Some(Sampler::Linear));
    }

    #[test]
    fn category_expected_kind_matches_media_family() {
        assert_eq!(Category::Sprite.expected_kind(), Kind::Image);
        assert_eq!(Category::Music.expected_kind(), Kind::Audio);
        assert_eq!(Category::Font.expected_kind(), Kind::Font);
        assert_eq!(Category::FontLicense.expected_kind(), Kind::Document);
    }

    #[test]
    fn rejects_an_unknown_status_value() {
        let toml = r#"
            version = 1
            [[record]]
            id = "x"
            path = "x.png"
            kind = "image"
            category = "sprite"
            status = "deprecated"
            provenance = "unknown"
            license = "unknown"
        "#;
        assert!(toml::from_str::<Sidecar>(toml).is_err());
    }
}
