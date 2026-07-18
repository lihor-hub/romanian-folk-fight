//! Focused validation diagnostics. Every variant carries the sidecar path,
//! the asset id (where one exists yet), and the violated field, per #167's
//! acceptance criteria.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Diagnostic {
    /// A `manifest.toml` could not be decoded as valid TOML matching the
    /// schema (bad syntax, wrong types, unknown enum values, ...).
    Undecodable { sidecar: PathBuf, error: String },
    /// A sidecar declared a `version` this build doesn't understand.
    UnsupportedVersion { sidecar: PathBuf, found: u32 },
    /// A `path`/`ignore.path` value contained a path separator, reaching
    /// outside the sidecar's own directory.
    PathEscapesSidecarDirectory {
        sidecar: PathBuf,
        id: String,
        path: String,
    },
    /// The same stable id appears in more than one record.
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
    /// Two records (or a record and an ignore) resolve to the same file.
    DuplicatePath {
        path: PathBuf,
        first: PathBuf,
        second: PathBuf,
    },
    /// A record or ignore entry references a file that does not exist
    /// on disk (not even under a different case).
    MissingFile {
        sidecar: PathBuf,
        id: String,
        path: PathBuf,
    },
    /// A record or ignore entry references a file that exists, but only
    /// under a different case somewhere in its path.
    CaseMismatch {
        sidecar: PathBuf,
        id: String,
        path: PathBuf,
    },
    /// A record has an empty/missing `license`.
    MissingLicense { sidecar: PathBuf, id: String },
    /// A record's `kind`/`category` doesn't match its file extension, or its
    /// `category` doesn't match its declared `kind`.
    WrongClassification {
        sidecar: PathBuf,
        id: String,
        field: &'static str,
        detail: String,
    },
    /// A record is missing a field that its `kind`/`category`/`status`
    /// combination requires.
    MissingRequiredField {
        sidecar: PathBuf,
        id: String,
        field: &'static str,
    },
    /// A file under `assets/` is not covered by any record or ignore entry
    /// in any sidecar.
    UncoveredFile { path: PathBuf },
    /// An `ignore` entry's `path` doesn't correspond to any file on disk
    /// (stale documentation).
    StaleIgnore { sidecar: PathBuf, path: PathBuf },
    /// `assets/CREDITS.md` doesn't mention a path it should, or mentions it
    /// without the sidecar-declared license nearby.
    CreditsDrift {
        id: String,
        path: PathBuf,
        detail: String,
    },
    /// Runtime character-catalog metadata disagrees with the registered
    /// sidecar asset contract.
    CatalogContent {
        catalog: PathBuf,
        part_id: String,
        detail: String,
    },

    // --- #185: runtime-reference and image-integrity validation ---
    /// Production Rust/HTML code references an asset whose sidecar record
    /// has `status` other than `runtime`, and no
    /// `validate::refs::RUNTIME_REFERENCE_EXEMPTIONS` entry covers it.
    IllegalRuntimeReference {
        file: PathBuf,
        line: usize,
        reference: String,
        id: String,
        status: String,
    },
    /// Production Rust/HTML code references a path with no sidecar record
    /// (or ignore entry) at all.
    UnresolvedRuntimeReference {
        file: PathBuf,
        line: usize,
        reference: String,
    },
    /// A `RUNTIME_REFERENCE_EXEMPTIONS` entry no longer matches any
    /// discovered production reference -- keeps the list honest, mirroring
    /// `StaleIgnore`.
    StaleRuntimeReferenceExemption { id: String },
    /// A record's recorded `dimensions` do not match the image's actual
    /// decoded pixel size.
    DimensionMismatch {
        sidecar: PathBuf,
        id: String,
        recorded: [u32; 2],
        actual: [u32; 2],
    },
    /// An image file exists and is covered by a record, but could not be
    /// decoded as a valid raster image.
    ImageDecodeError {
        sidecar: PathBuf,
        id: String,
        path: PathBuf,
        error: String,
    },
    /// A rig-attachment record's `crop` rectangle (when known -- i.e. not
    /// the literal `"unknown"`) is malformed or falls outside its source
    /// sheet's bounds.
    CropOutOfBounds {
        sidecar: PathBuf,
        id: String,
        crop: String,
        detail: String,
    },
    /// A rig-attachment record's `pivot` is implausibly far from its part's
    /// own `dimensions` -- almost certainly a data-entry error rather than
    /// a genuine rig offset.
    PivotOutOfBounds {
        sidecar: PathBuf,
        id: String,
        pivot: [f32; 2],
        dimensions: [u32; 2],
        tolerance: f32,
    },
    /// A rig-attachment record's `display` size is not a finite, positive
    /// width and height.
    InvalidDisplaySize {
        sidecar: PathBuf,
        id: String,
        display: [f32; 2],
    },
    /// A runtime image is fully transparent: every decoded pixel has
    /// alpha 0.
    EmptyAlpha {
        sidecar: PathBuf,
        id: String,
        path: PathBuf,
    },
    /// A runtime image contains chroma-key fringe pixels (magenta/green
    /// background-removal remnants) above the documented visibility
    /// tolerance.
    ChromaKeyFringe {
        sidecar: PathBuf,
        id: String,
        path: PathBuf,
        count: usize,
        max_alpha: u8,
    },
    /// A rig-attachment record's `display` aspect ratio is distorted from
    /// its source `dimensions` aspect ratio beyond the documented
    /// tolerance.
    AspectDistortion {
        sidecar: PathBuf,
        id: String,
        dimensions: [u32; 2],
        display: [f32; 2],
        ratio: f32,
        tolerance: f32,
    },
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Diagnostic::Undecodable { sidecar, error } => {
                write!(f, "{}: undecodable sidecar: {error}", sidecar.display())
            }
            Diagnostic::UnsupportedVersion { sidecar, found } => write!(
                f,
                "{}: unsupported schema version {found} (expected {})",
                sidecar.display(),
                crate::assets::schema::SCHEMA_VERSION
            ),
            Diagnostic::PathEscapesSidecarDirectory { sidecar, id, path } => write!(
                f,
                "{}: {id}: field `path` = {path:?} escapes the sidecar's own directory (must be a bare file name)",
                sidecar.display()
            ),
            Diagnostic::DuplicateId { id, first, second } => write!(
                f,
                "duplicate asset id {id:?}: declared in both {} and {}",
                first.display(),
                second.display()
            ),
            Diagnostic::DuplicatePath {
                path,
                first,
                second,
            } => write!(
                f,
                "duplicate path {}: covered by both {} and {}",
                path.display(),
                first.display(),
                second.display()
            ),
            Diagnostic::MissingFile { sidecar, id, path } => write!(
                f,
                "{}: {id}: field `path` = {} does not exist",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::CaseMismatch { sidecar, id, path } => write!(
                f,
                "{}: {id}: field `path` = {} exists only under a different case on disk",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::MissingLicense { sidecar, id } => write!(
                f,
                "{}: {id}: field `license` is missing or empty",
                sidecar.display()
            ),
            Diagnostic::WrongClassification {
                sidecar,
                id,
                field,
                detail,
            } => write!(
                f,
                "{}: {id}: field `{field}` is wrongly classified: {detail}",
                sidecar.display()
            ),
            Diagnostic::MissingRequiredField { sidecar, id, field } => write!(
                f,
                "{}: {id}: field `{field}` is required for this record's kind/category/status",
                sidecar.display()
            ),
            Diagnostic::UncoveredFile { path } => write!(
                f,
                "{} is not covered by any sidecar record or ignore entry",
                path.display()
            ),
            Diagnostic::StaleIgnore { sidecar, path } => write!(
                f,
                "{}: ignore entry for {} does not match any file on disk",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::CreditsDrift { id, path, detail } => {
                write!(f, "assets/CREDITS.md: {id} ({}): {detail}", path.display())
            }
            Diagnostic::CatalogContent {
                catalog,
                part_id,
                detail,
            } => write!(
                f,
                "{}: character part {part_id:?}: {detail}",
                catalog.display()
            ),
            Diagnostic::IllegalRuntimeReference {
                file,
                line,
                reference,
                id,
                status,
            } => write!(
                f,
                "{}:{line}: production reference {reference:?} resolves to asset `{id}` \
                 with field `status` = {status:?}, expected `runtime` (or a named entry in \
                 validate::refs::RUNTIME_REFERENCE_EXEMPTIONS)",
                file.display()
            ),
            Diagnostic::UnresolvedRuntimeReference {
                file,
                line,
                reference,
            } => write!(
                f,
                "{}:{line}: production reference {reference:?} does not resolve to any \
                 sidecar record or ignore entry under assets/",
                file.display()
            ),
            Diagnostic::StaleRuntimeReferenceExemption { id } => write!(
                f,
                "validate::refs::RUNTIME_REFERENCE_EXEMPTIONS: entry for asset `{id}` no \
                 longer matches any discovered production reference; remove it"
            ),
            Diagnostic::DimensionMismatch {
                sidecar,
                id,
                recorded,
                actual,
            } => write!(
                f,
                "{}: {id}: field `dimensions` = {recorded:?} does not match the image's actual \
                 decoded size {actual:?}",
                sidecar.display()
            ),
            Diagnostic::ImageDecodeError {
                sidecar,
                id,
                path,
                error,
            } => write!(
                f,
                "{}: {id}: field `path` = {} could not be decoded as a valid image: {error}",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::CropOutOfBounds {
                sidecar,
                id,
                crop,
                detail,
            } => write!(
                f,
                "{}: {id}: field `crop` = {crop:?} is out of bounds: {detail}",
                sidecar.display()
            ),
            Diagnostic::PivotOutOfBounds {
                sidecar,
                id,
                pivot,
                dimensions,
                tolerance,
            } => write!(
                f,
                "{}: {id}: field `pivot` = {pivot:?} exceeds the sanity bound of \
                 +/-{tolerance:.1} (derived from `dimensions` = {dimensions:?}); expected both \
                 components within +/-{tolerance:.1}",
                sidecar.display()
            ),
            Diagnostic::InvalidDisplaySize {
                sidecar,
                id,
                display,
            } => write!(
                f,
                "{}: {id}: field `display` = {display:?} is not a finite, positive width and \
                 height",
                sidecar.display()
            ),
            Diagnostic::EmptyAlpha { sidecar, id, path } => write!(
                f,
                "{}: {id}: field `path` = {} decodes to an image whose pixels are all fully \
                 transparent (alpha 0 everywhere), expected at least one visible pixel",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::ChromaKeyFringe {
                sidecar,
                id,
                path,
                count,
                max_alpha,
            } => write!(
                f,
                "{}: {id}: field `path` = {} contains {count} chroma-key-colored pixel(s) \
                 (magenta/green, max alpha {max_alpha}/255) at or above the visibility floor; \
                 expected no chroma-key remnants above that floor",
                sidecar.display(),
                path.display()
            ),
            Diagnostic::AspectDistortion {
                sidecar,
                id,
                dimensions,
                display,
                ratio,
                tolerance,
            } => write!(
                f,
                "{}: {id}: field `display` = {display:?} distorts the aspect ratio of \
                 `dimensions` = {dimensions:?} by a factor of {ratio:.2}x, expected within \
                 [{:.2}x, {tolerance:.2}x]",
                sidecar.display(),
                1.0 / tolerance
            ),
        }
    }
}
