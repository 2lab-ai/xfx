//! The one thing that writes `~/.xfx/settings.json`.
//!
//! Two properties make this a module rather than a function on a command:
//!
//! 1. **Read-repair never rewrites.** The new keys appear only here, because
//!    only a command the operator ran to change something is allowed to change
//!    the file. Loading, `status` and `doctor` leave it byte-identical.
//! 2. **Rollback is decided by the writer.** A v0.1.0 binary reads only the keys
//!    it knows, so it silently ignores `provider` and `models` and falls back to
//!    `backend` and `model`. That is safe **only if the writer keeps those two
//!    in sync**, which is what this does for as long as the selected provider
//!    has a value an older binary can reach. When it does not, the legacy keys
//!    are left at their previous values rather than given invented ones: an old
//!    binary then keeps talking to the backend it was last told about, which is
//!    a previously operator-chosen endpoint and never a compiled default.
//!
//! **Atomicity scope:** File replacement is atomic (stage + rename), so a reader
//! never sees a half-written file. However, the read-modify-write transaction is
//! NOT atomic: concurrent `xfx setup` invocations are last-writer-wins and may
//! lose the other setup's `models{}` or `llmux_url` updates. Recovery is simple:
//! re-run `xfx setup` to overwrite with the intended configuration.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::provider::ProviderId;

/// A provider and model selected for the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<'a> {
    pub provider: ProviderId,
    pub model: &'a str,
    pub llmux_url: Option<&'a str>,
}

/// Merges a selection into a settings document, keeping legacy keys in sync.
///
/// The rule for rollback is this: if the selected provider has a legacy value
/// an older binary can reach, the legacy `backend` and `model` keys are updated
/// to match the new ones. When the provider has no such value, the legacy keys
/// are left at their previous values.
///
/// Data preservation rule: if there is a flat `model` with a recognizable `backend`,
/// and the backend provider is NOT yet in the `models` object, migrate it there.
/// This prevents data loss when switching providers while a flat model from a
/// previous setup is still present.
pub fn merge_selection(
    mut existing: Map<String, Value>,
    selection: &Selection<'_>,
    legacy: Option<&str>,
) -> Map<String, Value> {
    // Write the new keys.
    existing.insert(
        crate::config::PROVIDER_KEY.to_string(),
        Value::String(selection.provider.label().to_string()),
    );

    // Ensure `models` is an object. Migrate flat `model` to its owning provider if needed.
    {
        let models_key = crate::config::MODELS_KEY.to_string();
        let mut models_obj =
            if existing.contains_key(&models_key) && existing[&models_key].is_object() {
                existing[&models_key].as_object().unwrap().clone()
            } else {
                Map::new()
            };

        // Migrate flat `model` if it exists with a recognizable `backend`
        // AND that backend provider is NOT yet in the models object.
        if let (Some(flat_model), Some(backend)) = (
            existing.get("model").and_then(Value::as_str),
            existing.get("backend").and_then(Value::as_str),
        ) {
            if let Some(backend_provider) = ProviderId::parse(backend) {
                let backend_label = backend_provider.label().to_string();
                // Only migrate if this provider is not already in models.
                if !models_obj.contains_key(&backend_label) {
                    models_obj.insert(backend_label, Value::String(flat_model.to_string()));
                }
            }
        }

        // Insert the new selection's model into its provider's entry.
        models_obj.insert(
            selection.provider.label().to_string(),
            Value::String(selection.model.to_string()),
        );
        existing.insert(models_key, Value::Object(models_obj));
    }

    // Update legacy keys if the provider has a legacy value.
    if let Some(backend) = legacy {
        existing.insert(
            crate::config::BACKEND_KEY.to_string(),
            Value::String(backend.to_string()),
        );
        existing.insert(
            "model".to_string(),
            Value::String(selection.model.to_string()),
        );
    }

    // Update llmux_url if provided.
    if let Some(url) = selection.llmux_url {
        existing.insert("llmux_url".to_string(), Value::String(url.to_string()));
    }

    existing
}

/// Writes `path` atomically and privately, merging the selection into it.
///
/// Follows the same discipline as `setup`: the stage lives in the target
/// directory so the rename cannot cross a filesystem, it is opened `create_new`
/// so a name this write did not create is never written through, the file is
/// created `0600` rather than tightened afterwards, and the directory is synced
/// so the *name* is durable.
pub fn write(
    path: &Path,
    existing: Map<String, Value>,
    selection: &Selection<'_>,
) -> io::Result<()> {
    write_document(path, &document_for(existing, selection))
}

/// The exact bytes a [`Selection`] merged into `existing` becomes on disk.
///
/// **The one serializer.** `write` used to merge and serialize inline, which
/// meant a caller could not know what the write would produce without
/// performing it -- and a caller that has to *decide* whether to keep a change
/// (`super::setup::prepare`) has to know exactly that. Split out rather than
/// duplicated, so `prepare` cannot promise one document while `commit` writes
/// another: there is only one document.
pub fn document_for(existing: Map<String, Value>, selection: &Selection<'_>) -> Vec<u8> {
    let merged = merge_selection(existing, selection, selection.provider.legacy_backend());
    let mut body = serde_json::to_string_pretty(&Value::Object(merged))
        .expect("a settings object is always serializable");
    body.push('\n');
    body.into_bytes()
}

/// Writes `document` to `path` with the profile's own discipline.
///
/// Everything `write` used to do after it had decided the bytes: the profile
/// home is created owner-only if it is not there, and the file is replaced
/// through a staged `create_new` and a rename.
pub fn write_document(path: &Path, document: &[u8]) -> io::Result<()> {
    let dir = parent_of(path)?;
    create_private_dir(dir)?;
    replace_private_file(dir, path, document)
}

/// The settings file as it stood before a transaction touched it.
///
/// Two states rather than one, and the difference is the whole point:
/// [`read_existing`] answers an empty map both for a file that is not there and
/// for a file that says `{}`, which is exactly right for *merging* and exactly
/// wrong for *rolling back*. Putting `{}` back where there had been no file
/// would leave a profile the operator never created, and every later `setup`
/// would merge into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilePreimage {
    /// There was no file at this path.
    Missing,
    /// There was one, with these bytes and these permission bits.
    ///
    /// The bytes are raw and unparsed on purpose: a preimage is what has to be
    /// put back, not what xfx understood. A file this build cannot parse is
    /// still a file whose operator gets it back unchanged.
    Present {
        bytes: Vec<u8>,
        /// The Unix permission bits, or `0` where there are none. On a platform
        /// without them restoration is bytes-only, which is the whole of the
        /// difference.
        mode: u32,
    },
}

/// Reads the preimage of `path`, before anything else touches disk.
pub fn snapshot(path: &Path) -> io::Result<ProfilePreimage> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(ProfilePreimage::Missing),
        Err(err) => return Err(err),
    };
    // A directory or a device where the settings file belongs is not something
    // this module may replace and then claim it could put back.
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "it is not a regular file",
        ));
    }
    let bytes = fs::read(path)?;
    Ok(ProfilePreimage::Present {
        bytes,
        mode: mode_of(&metadata),
    })
}

/// Puts `preimage` back at `path`.
///
/// `Missing` unlinks -- and only ever the path the transaction created, which
/// is why the caller must have proven the current bytes are the ones it wrote
/// before calling this -- then syncs the parent so the *absence* is durable.
/// `Present` goes through the same staged `create_new` and rename
/// [`replace_private_file`] uses, with the stage created at the recorded mode,
/// so a `0644` profile does not come back `0600`: restoration puts a file back,
/// it does not take the opportunity to tighten one.
pub fn restore(path: &Path, preimage: &ProfilePreimage) -> io::Result<()> {
    let dir = parent_of(path)?;
    match preimage {
        ProfilePreimage::Missing => {
            match fs::remove_file(path) {
                Ok(()) => {}
                // Already gone is the state this asks for, so it is not a
                // failure: a rollback reporting an error for having nothing to
                // undo would mask the failure it is unwinding.
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
            fault::trip(fault::Boundary::ParentSync)?;
            sync_directory(dir)
        }
        ProfilePreimage::Present { bytes, mode } => {
            replace_private_file_at(dir, path, bytes, *mode)
        }
    }
}

/// The directory a settings path lives in.
fn parent_of(path: &Path) -> io::Result<&Path> {
    path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the settings path has no parent directory",
        )
    })
}

/// The permission bits of `metadata`, or `0` on a platform without them.
fn mode_of(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

/// The settings object already on disk, or an empty one when there is none.
///
/// A file that exists and cannot be read is an error, never an empty object.
/// "xfx could not parse this" and "this is not worth keeping" are different
/// claims, and only the operator gets to make the second one.
pub fn read_existing(path: &Path) -> io::Result<Map<String, Value>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(err) => return Err(err),
    };
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "it is not a regular file",
        ));
    }
    if metadata.len() > crate::config::MAX_SETTINGS_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "file is larger than {} bytes",
                crate::config::MAX_SETTINGS_BYTES
            ),
        ));
    }
    let text = fs::read_to_string(path)?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "it is not a JSON object",
        )),
        Err(err) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("it is not valid JSON: {err}"),
        )),
    }
}

/// Creates the profile home owner-only, if it is not already there.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        // Created `0700` rather than created and then tightened: between the two
        // there would be a window in which the profile home is world-readable.
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

/// A stage name no other writer can own.
///
/// This process's id **and** a fresh nonce, which is the session store's shape.
/// The pid alone is not unique: two writes from this same process share it, so
/// the second wrote through a file the first was still filling.
fn stage_path(dir: &Path) -> PathBuf {
    // Taken by characters rather than sliced by byte index. `new_identifier`
    // returns lowercase hex today, so the two agree -- but a byte slice that is
    // only correct because of what another module happens to return is a panic
    // waiting for that module to change, in the one code path whose job is not
    // to destroy a settings file.
    let nonce: String = crate::session::new_identifier().chars().take(16).collect();
    dir.join(format!(
        "settings.json.{}.{nonce}{}",
        std::process::id(),
        crate::session::STAGE_SUFFIX
    ))
}

/// A staged file that removes itself unless it was renamed into place.
///
/// The store's guard, for the store's reason. The stage exists for the few
/// microseconds between "written" and "renamed", and if anything in between
/// fails the partial file must not be left behind to be mistaken for state --
/// but it must not be cleaned up by deleting a *fixed* name either, because a
/// fixed name is something another process might own.
struct StagedFile {
    path: PathBuf,
    committed: bool,
}

impl StagedFile {
    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.committed {
            // Only ever this write's own uniquely named stage.
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Writes `bytes` to `path` atomically and privately.
///
/// The discipline is the session store's: the stage lives in the target
/// directory so the rename cannot cross a filesystem, it is opened `create_new`
/// so a name this write did not create is never written through, the file is
/// created `0600` rather than tightened afterwards, and the directory is synced
/// so the *name* is durable and not only the bytes it points at.
///
/// A reader of `settings.json` therefore sees either the old document or the new
/// one -- never a half-written file. Note: this is file-replacement atomicity.
/// The read-modify-write transaction that calls this is NOT atomic; concurrent
/// writers are last-writer-wins (recoverable by re-running the operation).
pub fn replace_private_file(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace_private_file_at(dir, path, bytes, PRIVATE_MODE)
}

/// The permission bits a settings file this module *creates* is born with.
const PRIVATE_MODE: u32 = 0o600;

/// [`replace_private_file`], at an explicit mode.
///
/// The mode is a parameter for exactly one caller -- [`restore`], putting back
/// a preimage whose bits were not `0600`. Every other write goes through the
/// wrapper above and is born owner-only; a restoration is the one case where
/// the right answer is "whatever it was", because the operator's own `0644`
/// profile is theirs and not this module's to tighten on the way past.
fn replace_private_file_at(dir: &Path, path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let staged = StagedFile {
        path: stage_path(dir),
        committed: false,
    };
    {
        let mut options = fs::OpenOptions::new();
        // `create_new`, never `create().truncate()`: writing through a name this
        // write did not create is how a concurrent writer's in-flight file gets
        // destroyed.
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Created at the mode rather than chmod-ed afterwards: between the
            // two there would be a window in which a profile is readable by
            // more of the machine than it should be.
            options.mode(if mode == 0 { PRIVATE_MODE } else { mode });
        }
        #[cfg(not(unix))]
        let _ = mode;
        let mut file = options.open(&staged.path)?;
        io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    fault::trip(fault::Boundary::Rename)?;
    // A rename onto the target replaces its inode atomically. On failure the
    // guard removes this write's own stage on the way out -- and only that one.
    fs::rename(&staged.path, path)?;
    staged.commit();
    fault::trip(fault::Boundary::DirectorySync)?;
    sync_directory(dir)
}

/// The three boundaries a settings write can be made to fail at.
///
/// A transaction that classifies a failure by rereading the disk
/// (`super::setup`'s commit step) is only testable if the failure can be put
/// **at a named point** -- "before the rename" and "after the rename" are
/// different facts about the file, and an error value cannot be trusted to tell
/// them apart, which is the whole reason the classification rereads. Compiled
/// only under `fault-injection`, like `crate::tui::fault`, so a released binary
/// contains neither the hook nor the branch that consults it: there is no
/// environment variable a user can set to make their profile write fail.
///
/// Armed per **thread** rather than per process, because these are lib unit
/// tests and `cargo test` runs them in parallel: a process-wide switch would
/// make one test's injected failure land inside another test's write.
#[cfg(feature = "fault-injection")]
pub mod fault {
    use std::cell::Cell;
    use std::io;

    /// Where a settings write can be asked to fail.
    ///
    /// Each names the step it sits **in front of**, so the disk state at the
    /// moment of failure is exactly the state before that step ran.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Boundary {
        /// The stage is written and synced; the rename has not happened. Disk
        /// still holds the preimage.
        Rename,
        /// The rename has landed and the directory has not been synced. Disk
        /// holds the new document.
        DirectorySync,
        /// A `Missing` restore has unlinked the file and not yet synced the
        /// parent.
        ParentSync,
    }

    impl Boundary {
        fn detail(self) -> &'static str {
            match self {
                Self::Rename => "injected failure before the rename",
                Self::DirectorySync => "injected failure before the directory sync",
                Self::ParentSync => "injected failure before the parent sync",
            }
        }
    }

    thread_local! {
        static ARMED: Cell<Option<Boundary>> = const { Cell::new(None) };
    }

    /// Asks the next write on **this thread** to fail at `boundary`, once.
    pub fn arm(boundary: Boundary) {
        ARMED.with(|armed| armed.set(Some(boundary)));
    }

    /// Disarms whatever was armed, so a test cannot leak a fault into the next.
    pub fn disarm() {
        ARMED.with(|armed| armed.set(None));
    }

    /// Fails if this thread asked to fail here. One-shot: the arming is
    /// consumed, so a rollback's own rename is not caught by the fault that
    /// made the commit fail.
    pub(super) fn trip(boundary: Boundary) -> io::Result<()> {
        let hit = ARMED.with(|armed| {
            if armed.get() == Some(boundary) {
                armed.set(None);
                true
            } else {
                false
            }
        });
        if hit {
            return Err(io::Error::other(boundary.detail()));
        }
        Ok(())
    }
}

/// The same three boundaries, compiled to nothing.
///
/// A release build has no enum, no thread-local and no branch -- the calls above
/// become `Ok(())` at every site and vanish. A hook reachable in a release build
/// would be a defect, so the seam is a compile-time one rather than a flag.
#[cfg(not(feature = "fault-injection"))]
mod fault {
    use std::io;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Boundary {
        Rename,
        DirectorySync,
        ParentSync,
    }

    #[inline(always)]
    pub(super) fn trip(boundary: Boundary) -> io::Result<()> {
        let _ = boundary;
        Ok(())
    }
}

/// Flushes the directory entry, so the rename survives a crash.
fn sync_directory(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().expect("an object").clone()
    }

    #[test]
    fn a_selection_writes_the_new_keys_and_keeps_the_legacy_pair_in_sync() {
        // Rollback is the dangerous direction and it is decided here: a v0.1.0
        // binary ignores `provider` and `models`, so the two keys it does read
        // have to still say the same thing.
        let merged = merge_selection(
            object(json!({"permission_mode": "auto"})),
            &Selection {
                provider: ProviderId::Llmux,
                model: "fable",
                llmux_url: Some("http://127.0.0.1:3456"),
            },
            ProviderId::Llmux.legacy_backend(),
        );
        assert_eq!(merged["provider"], "llmux");
        assert_eq!(merged["models"]["llmux"], "fable");
        assert_eq!(merged["backend"], "llmux");
        assert_eq!(merged["model"], "fable");
        assert_eq!(merged["llmux_url"], "http://127.0.0.1:3456");
        assert_eq!(merged["permission_mode"], "auto", "unrelated keys survive");
    }

    #[test]
    fn another_providers_model_preference_is_preserved() {
        // Switching away from a provider must not lose what was chosen for it.
        let merged = merge_selection(
            object(json!({"models": {"gateway": "zai/glm-5.2"}})),
            &Selection {
                provider: ProviderId::Llmux,
                model: "fable",
                llmux_url: None,
            },
            ProviderId::Llmux.legacy_backend(),
        );
        assert_eq!(merged["models"]["gateway"], "zai/glm-5.2");
        assert_eq!(merged["models"]["llmux"], "fable");
    }

    #[test]
    fn a_models_value_that_was_not_an_object_is_replaced_rather_than_merged_into() {
        // There is nothing to preserve in a value that was never a map, and
        // trying to merge into it would fail the write over someone else's typo.
        let merged = merge_selection(
            object(json!({"models": "nonsense"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "zai/glm-5.2",
                llmux_url: None,
            },
            ProviderId::Gateway.legacy_backend(),
        );
        assert_eq!(merged["models"], json!({"gateway": "zai/glm-5.2"}));
    }

    #[test]
    fn a_provider_no_older_binary_can_reach_leaves_the_legacy_keys_alone() {
        // The rule for a provider with no representable legacy value: leave
        // `backend` at its previous value rather than inventing one, so an old
        // binary keeps talking to the backend it was last told about instead of
        // to a provider it cannot authenticate. No such provider exists in this
        // build, which is why the rule is proven on the pure helper.
        let merged = merge_selection(
            object(json!({"backend": "llmux", "model": "fable"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "some-future-model",
                llmux_url: None,
            },
            None,
        );
        assert_eq!(merged["backend"], "llmux", "untouched");
        assert_eq!(merged["model"], "fable", "untouched");
        assert_eq!(merged["provider"], "gateway");
        assert_eq!(merged["models"]["gateway"], "some-future-model");
    }

    // -----------------------------------------------------------------------
    // the preimage, and the document as a value
    // -----------------------------------------------------------------------

    fn selection() -> Selection<'static> {
        Selection {
            provider: ProviderId::Llmux,
            model: "fable",
            llmux_url: Some("http://127.0.0.1:3456"),
        }
    }

    #[test]
    fn snapshot_distinguishes_a_missing_profile_from_an_empty_one() {
        // The distinction `read_existing` throws away, and the whole reason a
        // preimage is a type rather than a `Vec<u8>`: a rollback that put back
        // `{}` where there had been *no file* would leave a profile behind that
        // the operator never had, and every later `setup` would merge into it.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        assert_eq!(snapshot(&path).expect("absent"), ProfilePreimage::Missing);

        fs::write(&path, b"{}\n").expect("write an empty object");
        let present = snapshot(&path).expect("present");
        assert_ne!(present, ProfilePreimage::Missing);
        match present {
            ProfilePreimage::Present { bytes, .. } => assert_eq!(bytes, b"{}\n"),
            ProfilePreimage::Missing => unreachable!(),
        }
        // And the collapse it is contrasted with, on the same two paths.
        let empty_dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(
            read_existing(&empty_dir.path().join("settings.json")).expect("absent"),
            read_existing(&path).expect("present"),
            "read_existing answers the same map for both, which is why it cannot roll back"
        );
    }

    #[test]
    fn restore_of_a_missing_preimage_unlinks_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let preimage = snapshot(&path).expect("absent");
        write_document(&path, &document_for(Map::new(), &selection())).expect("write");
        assert!(path.exists(), "the transaction created it");

        restore(&path, &preimage).expect("restore");
        assert!(
            !path.exists(),
            "nothing this transaction created is left behind"
        );
        // Idempotent: a rollback that ran twice must not become an error, or a
        // failure on the way out would be reported as a second failure.
        restore(&path, &preimage).expect("restoring an already-absent file");
    }

    #[cfg(unix)]
    #[test]
    fn restore_of_a_present_preimage_returns_exact_bytes_and_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let original = b"{\n  \"permission_mode\": \"auto\"\n}\n";
        fs::write(&path, original).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let preimage = snapshot(&path).expect("snapshot");
        write_document(&path, &document_for(Map::new(), &selection())).expect("write");
        assert_ne!(fs::read(&path).expect("read"), original, "the write landed");
        assert_eq!(
            fs::metadata(&path).expect("stat").permissions().mode() & 0o777,
            0o600,
            "the writer creates 0600"
        );

        restore(&path, &preimage).expect("restore");
        assert_eq!(fs::read(&path).expect("read"), original, "byte-exact");
        assert_eq!(
            fs::metadata(&path).expect("stat").permissions().mode() & 0o777,
            0o644,
            "a 0644 preimage stays 0644 -- restoration is not a second tightening"
        );
    }

    #[test]
    fn the_document_is_exactly_what_the_writer_puts_on_disk() {
        // One serializer, not two. `prepare` can only promise the bytes
        // `commit` will write if the same function produced them.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let existing = object(json!({"permission_mode": "auto"}));
        let document = document_for(existing.clone(), &selection());
        write(&path, existing, &selection()).expect("write");
        assert_eq!(fs::read(&path).expect("read"), document);
        assert!(
            document.ends_with(b"\n"),
            "pretty JSON with a trailing newline"
        );
    }

    #[test]
    fn a_selection_without_a_url_does_not_erase_a_recorded_one() {
        let merged = merge_selection(
            object(json!({"llmux_url": "http://127.0.0.1:3456"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "zai/glm-5.2",
                llmux_url: None,
            },
            ProviderId::Gateway.legacy_backend(),
        );
        assert_eq!(merged["llmux_url"], "http://127.0.0.1:3456");
    }
}
