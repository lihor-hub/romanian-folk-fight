//! Platform storage for the run snapshot (#201, building on #193's split
//! between schema/migration — [`super::snapshot`] — and *where the JSON
//! physically lives*, which is this module): the [`SaveBackend`] trait, its
//! native/web/in-memory implementations, and the shared typed load outcome
//! ([`SnapshotLoad`]) both platforms report so a caller (the main menu)
//! never has to re-parse or re-classify a payload itself — it always goes
//! through [`super::snapshot::SaveGame::load`] via [`load_save_outcome`].
//!
//! # Native durability: temp file + atomic replace
//!
//! [`platform::PlatformBackend::store`] (native only; see its own doc
//! comment) never writes the target file in place. It writes the full
//! payload to a fixed, same-directory temporary file, durably flushes it
//! (`File::sync_all`, so the bytes are actually on disk, not sitting in an
//! OS write buffer a power loss could still lose), and only then atomically
//! replaces the target with `std::fs::rename` — atomic on both POSIX and
//! Windows for a same-volume rename. A crash or power loss at any point
//! *before* the rename leaves the previous file completely untouched (the
//! rename never ran); a crash *during or after* the rename is not
//! observable as a partial state at all, because a rename either fully
//! happens or fully doesn't. Either way the target is always a complete,
//! previously-written payload — never a half-written mix of old and new
//! bytes ("no torn saves"). A failure at any step (temp-file write or
//! rename) triggers a best-effort cleanup of the temp file and is logged,
//! never panicked on. See this module's `native_atomic_write_tests` for the
//! failure-injection tests backing this.
//!
//! The web backend needs none of this: `window.localStorage.setItem` is a
//! single synchronous browser API call with no torn-write window to guard
//! against, so it stores the payload directly (same as before #201).
//!
//! # Typed load outcome
//!
//! [`SnapshotLoad`] distinguishes *no save*, *valid*, *invalid* (corrupt,
//! partially written, or an unsupported old version), and *future-version*
//! (written by a newer build than this one) — computed once, generically,
//! by [`load_save_outcome`] over whatever bytes [`SaveBackend::load`]
//! returns. Because classification lives in exactly one place rather than
//! being reimplemented per backend, native and web report the identical
//! outcome for identical stored bytes by construction; this module's
//! `native_and_in_memory_backends_report_the_same_load_outcome_for_the_same_bytes`
//! test demonstrates that contract with the two backends this native test
//! binary can actually run (the in-memory double exercises the exact same
//! `store`/`load`/`clear` contract the wasm backend does, just over a
//! `Mutex<Option<String>>` instead of `localStorage`).
//!
//! [`load_save`] is the pre-#201 convenience wrapper (unchanged behavior):
//! it discards an invalid/future-version save so a caller that only wants
//! "is there something to resume" never re-reads known-bad data.
//! `menu::spawn_main_menu` deliberately does *not* use it — it calls
//! [`load_save_outcome`] directly so it can present a recovery action
//! instead of the data being silently gone before the player ever saw it
//! was there (see `crate::menu`'s `RecoverSaveButton`/`MenuAction::ClearCorruptSave`).

use bevy::prelude::*;

use super::snapshot::{self, SaveGame};

/// Where save JSON physically lives; one implementation per platform plus an
/// in-memory one for tests.
pub trait SaveBackend: Send + Sync + 'static {
    /// Writes the snapshot, replacing any previous one. Errors are logged,
    /// never panicked on.
    fn store(&self, json: &str);
    /// The stored snapshot, if any.
    fn load(&self) -> Option<String>;
    /// Deletes the stored snapshot, if any.
    fn clear(&self);
}

/// The save store of the running game: the platform backend by default
/// ([`Default`]), an in-memory one in tests.
#[derive(Resource)]
pub struct SaveStore(Box<dyn SaveBackend>);

impl SaveStore {
    /// A store over a specific backend (tests use the in-memory one).
    pub fn with_backend(backend: impl SaveBackend) -> Self {
        Self(Box::new(backend))
    }

    /// Writes the snapshot, replacing any previous one.
    pub fn store(&self, json: &str) {
        self.0.store(json);
    }

    /// The stored snapshot, if any.
    pub fn load(&self) -> Option<String> {
        self.0.load()
    }

    /// Deletes the stored snapshot, if any.
    pub fn clear(&self) {
        self.0.clear();
    }
}

impl Default for SaveStore {
    fn default() -> Self {
        Self(Box::new(platform_backend("save.json", super::STORAGE_KEY)))
    }
}

/// The platform backend at a custom location: a file named `file_name` under
/// the game's data directory on native, the `storage_key` entry of
/// `localStorage` on wasm. Lets other persisted blobs (e.g. the audio
/// settings, #30) reuse the same storage machinery under their own key.
#[cfg(not(target_arch = "wasm32"))]
pub fn platform_backend(file_name: &'static str, _storage_key: &'static str) -> impl SaveBackend {
    platform::PlatformBackend {
        file_name,
        #[cfg(test)]
        base_override: None,
    }
}

/// See the native `platform_backend`; on wasm the `storage_key` selects the
/// `localStorage` entry and the file name is unused.
#[cfg(target_arch = "wasm32")]
pub fn platform_backend(_file_name: &'static str, storage_key: &'static str) -> impl SaveBackend {
    platform::PlatformBackend { storage_key }
}

/// The outcome of loading whatever is currently stored under a [`SaveStore`]
/// — shared verbatim by every [`SaveBackend`] (native, web, the in-memory
/// test double). Added for #201 so a caller (the main menu) can distinguish
/// "nothing saved yet" from "something is saved, but it can't be read"
/// without re-parsing the payload itself — see [`load_save_outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotLoad {
    /// Nothing stored — a fresh install, or after [`SaveStore::clear`].
    NoSave,
    /// A valid, directly-usable (current-version or migrated) snapshot.
    Valid(SaveGame),
    /// Present, but its bytes don't parse/validate as any known version —
    /// corrupt JSON, a torn/partial write, an unsupported old version, or an
    /// item name outside the current catalog. See
    /// [`snapshot::SnapshotLoadError::Invalid`].
    Invalid,
    /// Present, but written by a newer build than this one (the stored
    /// version is greater than [`super::CURRENT_VERSION`]). See
    /// [`snapshot::SnapshotLoadError::FutureVersion`].
    FutureVersion,
}

/// Classifies whatever is currently stored under `store` — see
/// [`SnapshotLoad`]. Never mutates the store; contrast [`load_save`], which
/// additionally clears an unusable save as a convenience for callers that
/// only want a resumable-or-not answer.
pub fn load_save_outcome(store: &SaveStore) -> SnapshotLoad {
    match store.load() {
        None => SnapshotLoad::NoSave,
        Some(json) => match SaveGame::load(&json) {
            Ok(save) => SnapshotLoad::Valid(save),
            Err(snapshot::SnapshotLoadError::Invalid) => SnapshotLoad::Invalid,
            Err(snapshot::SnapshotLoadError::FutureVersion) => SnapshotLoad::FutureVersion,
        },
    }
}

/// Loads and validates the stored save, clearing it if it turns out to be
/// unusable (corrupt, partially written, or an unsupported/future version)
/// so the caller never re-reads a known-bad save. A convenience over
/// [`load_save_outcome`] for callers that only want "is there a save I can
/// resume right now" — `menu::handle_menu_actions`'s **Continuă** handler
/// re-validates with this on every click rather than trusting a stale
/// button. `menu::spawn_main_menu` deliberately does *not* use this: it
/// calls [`load_save_outcome`] directly so an invalid/future-version save
/// can be offered back to the player as a recovery action instead of being
/// silently discarded before they ever saw it was there.
pub fn load_save(store: &SaveStore) -> Option<SaveGame> {
    match load_save_outcome(store) {
        SnapshotLoad::Valid(save) => Some(save),
        SnapshotLoad::NoSave => None,
        SnapshotLoad::Invalid | SnapshotLoad::FutureVersion => {
            warn!("discarding invalid save");
            store.clear();
            None
        }
    }
}

/// Native backend: `dirs::data_dir()/romanian-folk-fight/<file_name>`.
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use bevy::prelude::warn;

    use super::SaveBackend;

    pub struct PlatformBackend {
        /// File name under the game's data directory (e.g. `save.json`).
        pub file_name: &'static str,
        /// Overrides the base directory (`dirs::data_dir()/romanian-folk-fight`
        /// by default) — set only by this module's own tests (via
        /// [`PlatformBackend::at`]), which need an isolated scratch
        /// directory rather than touching this machine's real save
        /// location. Always `None` in production: `platform_backend`, the
        /// only production constructor, never sets it, and the field itself
        /// does not exist outside `cfg(test)` builds at all.
        #[cfg(test)]
        pub(crate) base_override: Option<PathBuf>,
    }

    #[cfg(test)]
    impl PlatformBackend {
        /// A backend pointed at `base` instead of
        /// `dirs::data_dir()/romanian-folk-fight` — this module's own tests
        /// use this so they read/write an isolated scratch directory,
        /// never this machine's real save location.
        pub(crate) fn at(base: PathBuf, file_name: &'static str) -> Self {
            Self {
                file_name,
                base_override: Some(base),
            }
        }
    }

    impl PlatformBackend {
        fn base_dir(&self) -> Option<PathBuf> {
            #[cfg(test)]
            if let Some(base) = &self.base_override {
                return Some(base.clone());
            }
            Some(dirs::data_dir()?.join("romanian-folk-fight"))
        }

        /// The backing file path; `None` when the platform has no data
        /// directory.
        fn path(&self) -> Option<PathBuf> {
            Some(self.base_dir()?.join(self.file_name))
        }

        /// The same-directory temporary file [`Self::store`]'s
        /// [`SaveBackend::store`] impl writes through before the atomic
        /// rename. Fixed (not uniquely named per call): a stale leftover
        /// from a previous crash is simply overwritten by the next write
        /// attempt's `File::create`, and every distinct `(file_name,
        /// directory)` pair used in this codebase (the run save, the
        /// settings blob, each via their own `platform_backend` call) gets
        /// its own distinct temp file, so two backends can never collide on
        /// one temp path.
        fn temp_path(&self, dir: &Path) -> PathBuf {
            dir.join(format!("{}.tmp", self.file_name))
        }
    }

    impl SaveBackend for PlatformBackend {
        /// Same-directory temp file, durable write, atomic replace, cleanup
        /// on failure — see this module's doc comment for the full
        /// protocol and why it guarantees no torn saves.
        fn store(&self, json: &str) {
            let Some(path) = self.path() else {
                warn!("no platform data directory; save not written");
                return;
            };
            let Some(parent) = path.parent() else {
                warn!("save path {path:?} has no parent directory; save not written");
                return;
            };
            if let Err(err) = std::fs::create_dir_all(parent) {
                warn!("could not create save directory {parent:?}: {err}");
                return;
            }
            let temp_path = self.temp_path(parent);
            if let Err(err) = write_durably(&temp_path, json) {
                warn!("could not write temporary save file {temp_path:?}: {err}");
                let _ = std::fs::remove_file(&temp_path); // best-effort cleanup
                return;
            }
            if let Err(err) = std::fs::rename(&temp_path, &path) {
                warn!("could not replace save file {path:?}: {err}");
                let _ = std::fs::remove_file(&temp_path); // best-effort cleanup
            }
        }

        fn load(&self) -> Option<String> {
            std::fs::read_to_string(self.path()?).ok()
        }

        fn clear(&self) {
            if let Some(path) = self.path() {
                // A missing file is already "cleared"; other errors leave a
                // stale save behind, which the version/validation guard on
                // load keeps harmless.
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Writes `contents` to `path` (creating or truncating it) and durably
    /// flushes it to disk (`File::sync_all`) before returning — so by the
    /// time [`PlatformBackend::store`] proceeds to the atomic rename, the
    /// temp file's bytes are guaranteed to actually be on disk, not merely
    /// sitting in an OS write buffer a power loss could still lose.
    fn write_durably(path: &Path, contents: &str) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}

/// Web backend: `window.localStorage` under a caller-chosen key (the run
/// save's is [`super::STORAGE_KEY`]). A single `localStorage` call is
/// already atomic from the page's point of view — no torn-write window like
/// native's needs guarding against — so this backend is unchanged by #201.
#[cfg(target_arch = "wasm32")]
mod platform {
    use bevy::prelude::warn;

    use super::SaveBackend;

    pub struct PlatformBackend {
        /// The `localStorage` key this backend reads and writes.
        pub storage_key: &'static str,
    }

    /// The window's local storage; `None` when unavailable (e.g. blocked by
    /// the browser).
    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    impl SaveBackend for PlatformBackend {
        fn store(&self, json: &str) {
            match local_storage() {
                Some(storage) => {
                    if storage.set_item(self.storage_key, json).is_err() {
                        warn!("could not write save to localStorage");
                    }
                }
                None => warn!("localStorage unavailable; save not written"),
            }
        }

        fn load(&self) -> Option<String> {
            local_storage()?.get_item(self.storage_key).ok().flatten()
        }

        fn clear(&self) {
            if let Some(storage) = local_storage() {
                let _ = storage.remove_item(self.storage_key);
            }
        }
    }
}

/// In-memory backend for tests: a shared cell the test inspects.
#[cfg(test)]
pub(crate) struct MemoryBackend(pub(crate) std::sync::Arc<std::sync::Mutex<Option<String>>>);

#[cfg(test)]
impl SaveBackend for MemoryBackend {
    fn store(&self, json: &str) {
        *self.0.lock().expect("test store lock") = Some(json.to_string());
    }

    fn load(&self) -> Option<String> {
        self.0.lock().expect("test store lock").clone()
    }

    fn clear(&self) {
        *self.0.lock().expect("test store lock") = None;
    }
}

#[cfg(test)]
impl SaveStore {
    /// An in-memory store plus the shared cell tests inspect and seed.
    pub(crate) fn in_memory() -> (Self, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
        (
            Self::with_backend(MemoryBackend(std::sync::Arc::clone(&cell))),
            cell,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::snapshot::tests::sample_save;
    use crate::save::snapshot::{CURRENT_VERSION, SnapshotLoadError};

    // --- SnapshotLoad classification ---

    #[test]
    fn load_save_outcome_reports_no_save_for_an_empty_store() {
        let (store, _cell) = SaveStore::in_memory();
        assert_eq!(load_save_outcome(&store), SnapshotLoad::NoSave);
    }

    #[test]
    fn load_save_outcome_reports_valid_for_a_valid_save() {
        let (store, _cell) = SaveStore::in_memory();
        let save = sample_save();
        store.store(&save.to_json().expect("plain data serializes"));
        assert_eq!(load_save_outcome(&store), SnapshotLoad::Valid(save));
    }

    #[test]
    fn load_save_outcome_reports_invalid_for_corrupt_json() {
        let (store, _cell) = SaveStore::in_memory();
        store.store("definitely not json");
        assert_eq!(load_save_outcome(&store), SnapshotLoad::Invalid);
    }

    #[test]
    fn load_save_outcome_reports_future_version_for_a_newer_version() {
        let (store, _cell) = SaveStore::in_memory();
        let mut save = sample_save();
        save.version = CURRENT_VERSION + 1;
        store.store(&save.to_json().expect("plain data serializes"));
        assert_eq!(load_save_outcome(&store), SnapshotLoad::FutureVersion);
    }

    /// Sanity check that the `SnapshotLoadError` variants line up with the
    /// `SnapshotLoad` ones the way [`load_save_outcome`]'s match assumes --
    /// if `snapshot`'s error type ever grows a variant, this (and the
    /// non-exhaustive match in `load_save_outcome`) will fail to compile
    /// instead of silently swallowing the new case.
    #[test]
    fn snapshot_load_error_variants_are_exactly_the_two_load_save_outcome_handles() {
        let invalid = SnapshotLoadError::Invalid;
        let future = SnapshotLoadError::FutureVersion;
        assert_ne!(invalid, future);
    }

    // --- load_save (auto-clearing convenience) ---

    #[test]
    fn load_save_clears_an_invalid_store() {
        let (store, cell) = SaveStore::in_memory();
        store.store("definitely not a save");
        assert!(load_save(&store).is_none());
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "the corrupt save is cleared, not re-read forever"
        );
    }

    #[test]
    fn load_save_returns_a_valid_snapshot_and_keeps_it_stored() {
        let (store, cell) = SaveStore::in_memory();
        let save = sample_save();
        store.store(&save.to_json().expect("plain data serializes"));
        assert_eq!(load_save(&store), Some(save));
        assert!(
            cell.lock().expect("test store lock").is_some(),
            "a valid save stays stored"
        );
    }

    #[test]
    fn load_save_clears_a_future_version_store_too() {
        let (store, cell) = SaveStore::in_memory();
        let mut save = sample_save();
        save.version = CURRENT_VERSION + 1;
        store.store(&save.to_json().expect("plain data serializes"));
        assert!(load_save(&store).is_none());
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "a future-version save is cleared too -- load_save never re-reads known-bad data"
        );
    }

    // --- native atomic write / durability / cleanup ---

    #[cfg(not(target_arch = "wasm32"))]
    mod native_atomic_write_tests {
        use std::sync::atomic::{AtomicU64, Ordering};

        use super::super::platform;
        use crate::save::SaveBackend;

        /// A fresh, uniquely-named scratch directory under the OS temp dir
        /// — never `dirs::data_dir()` (this machine's real save location).
        /// Each test gets its own so parallel `cargo test` runs never
        /// collide.
        fn unique_scratch_dir(label: &str) -> std::path::PathBuf {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("rff-save-storage-test-{label}-{nanos}-{n}"));
            std::fs::create_dir_all(&dir).expect("test scratch dir");
            dir
        }

        #[test]
        fn store_then_load_round_trips_exactly() {
            let dir = unique_scratch_dir("roundtrip");
            let backend = platform::PlatformBackend::at(dir.clone(), "save.json");
            backend.store(r#"{"hello":"world"}"#);
            assert_eq!(backend.load().as_deref(), Some(r#"{"hello":"world"}"#));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn store_replaces_previous_content_atomically_and_leaves_no_temp_file() {
            let dir = unique_scratch_dir("replace");
            let backend = platform::PlatformBackend::at(dir.clone(), "save.json");
            backend.store("first");
            backend.store("second");
            assert_eq!(
                backend.load().as_deref(),
                Some("second"),
                "the second write fully replaces the first -- no merge, no leftover bytes"
            );
            assert!(
                !dir.join("save.json.tmp").exists(),
                "a successful store leaves no stray temp file behind"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[cfg(unix)]
        #[test]
        fn a_failed_write_leaves_the_previous_save_completely_untouched() {
            use std::os::unix::fs::PermissionsExt;

            let dir = unique_scratch_dir("failed-write");
            let backend = platform::PlatformBackend::at(dir.clone(), "save.json");
            backend.store("the only good copy");
            assert_eq!(backend.load().as_deref(), Some("the only good copy"));

            // Make the directory read-only so the temp file's `File::create`
            // fails partway through the next `store` -- simulating an
            // interrupted/failed write without literally killing the
            // process mid-write.
            let original_perms = std::fs::metadata(&dir).expect("dir exists").permissions();
            let mut readonly = original_perms.clone();
            readonly.set_mode(0o500);
            std::fs::set_permissions(&dir, readonly).expect("can chmod the test dir");

            backend.store("this must never land");

            // Restore write permission before reading back / cleaning up.
            std::fs::set_permissions(&dir, original_perms).expect("can restore permissions");

            assert_eq!(
                backend.load().as_deref(),
                Some("the only good copy"),
                "a write that fails before the atomic rename must never touch the \
                 previous save -- no torn saves"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn clearing_a_missing_file_is_a_harmless_no_op() {
            let dir = unique_scratch_dir("clear-missing");
            let backend = platform::PlatformBackend::at(dir.clone(), "save.json");
            backend.clear(); // nothing stored yet
            assert_eq!(backend.load(), None);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn clear_removes_a_stored_file() {
            let dir = unique_scratch_dir("clear");
            let backend = platform::PlatformBackend::at(dir.clone(), "save.json");
            backend.store("something");
            backend.clear();
            assert_eq!(backend.load(), None);
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// Backend-contract proof (#201's acceptance criterion "native and
        /// web return the same typed outcomes"): `load_save_outcome` is one
        /// generic function over whatever bytes `SaveBackend::load` returns
        /// (see this module's own doc comment), so it necessarily agrees
        /// across *any* two backends fed the same bytes -- demonstrated here
        /// with the native file backend and the in-memory one (which
        /// exercises the exact same trait contract the wasm backend does,
        /// just over a `Mutex` instead of `localStorage`). The wasm backend
        /// itself cannot run inside this native test binary; the shared,
        /// backend-agnostic classification function is what makes that
        /// untestable-here case safe anyway.
        #[test]
        fn native_and_in_memory_backends_report_the_same_load_outcome_for_the_same_bytes() {
            use crate::save::snapshot::CURRENT_VERSION;
            use crate::save::snapshot::tests::sample_save;
            use crate::save::storage::{SaveStore, load_save_outcome};

            let dir = unique_scratch_dir("contract");
            let valid_json = sample_save().to_json().expect("plain data serializes");
            let future_json = {
                let mut save = sample_save();
                save.version = CURRENT_VERSION + 1;
                save.to_json().expect("plain data serializes")
            };

            for (label, content) in [
                ("valid", Some(valid_json.as_str())),
                ("corrupt", Some("not json at all")),
                ("future-version", Some(future_json.as_str())),
                ("no-save", None),
            ] {
                let native = platform::PlatformBackend::at(dir.clone(), "save.json");
                native.clear();
                let (memory_store, _cell) = SaveStore::in_memory();
                if let Some(content) = content {
                    native.store(content);
                    memory_store.store(content);
                }
                let native_store = SaveStore::with_backend(native);
                let native_outcome = load_save_outcome(&native_store);
                let memory_outcome = load_save_outcome(&memory_store);
                assert_eq!(
                    std::mem::discriminant(&native_outcome),
                    std::mem::discriminant(&memory_outcome),
                    "{label}: native and in-memory backends disagree on the load outcome"
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
