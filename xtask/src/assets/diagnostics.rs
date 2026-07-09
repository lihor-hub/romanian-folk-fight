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
        }
    }
}
