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
//! never sees a half-written file. Every write is also a serialized
//! transaction: its read, change and rename happen under an in-process mutex
//! and upstream's cross-process `settings.lock`, so two writers of different
//! keys both survive -- a **preference** write ([`write_preferences`]) and
//! `xfx setup`'s ([`rewrite_document`]), which decides its selection before it
//! takes the lock and merges it into the file as the file stands once it has
//! the lock.

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

/// Sets one preference in the settings file at `path` and writes it back.
///
/// `key_path` names a key inside nested objects -- `["statusLine", "context"]`
/// is `statusLine.context` -- and `value` replaces whatever was there. Every
/// object on the way is created when it is missing, and a value on the way
/// that is not an object is **replaced** by one rather than refused: the
/// operator asked for this key to hold this value, and upstream's partial
/// merge does the same (`attemptUserPreferences`). Nothing else in the file is
/// touched, so a key this build does not know survives the write.
///
/// The read is [`read_existing`] and the write is [`write_document`], so a file
/// xfx cannot parse is an error rather than an empty object that would be
/// written over it, and the replacement is the profile's own staged, owner-only
/// rename. The bytes are pretty JSON and a newline, the same serializer
/// [`document_for`] uses. The whole of it is one transaction under
/// [`SettingsLock`] -- and this process's own writers queue for it -- so a key
/// another writer set in between is never lost; a lock another process holds
/// past two seconds is a failure (`WouldBlock`) rather than a write.
///
/// An empty `key_path` names no key and is refused before anything is read.
pub fn write_preference(path: &Path, key_path: &[&str], value: Value) -> io::Result<()> {
    write_preferences(path, &[(key_path, value)])
}

/// [`write_preference`] for several keys, as **one** read and one rename.
///
/// What a change that is one decision but two keys needs -- `/model`'s effort
/// column sets the model and the effort together -- so that a reader never sees
/// one without the other, and so the two cost one transaction rather than two.
/// Every key path is checked before anything is read.
pub fn write_preferences(path: &Path, preferences: &[(&[&str], Value)]) -> io::Result<()> {
    write_preferences_within(path, preferences, SETTINGS_LOCK_DEADLINE)
}

/// [`write_preferences`], waiting at most `deadline` for another process's
/// `settings.lock`.
fn write_preferences_within(
    path: &Path,
    preferences: &[(&[&str], Value)],
    deadline: std::time::Duration,
) -> io::Result<()> {
    if preferences.iter().any(|(key_path, _)| key_path.is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a preference needs a key",
        ));
    }
    // The whole transaction, read to rename, under both locks: this process's
    // first, so two threads here queue on a mutex rather than spinning on a
    // file lock, and then the one every other process honours.
    let _in_process = WRITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = parent_of(path)?;
    create_private_dir(dir)?;
    let _across_processes = SettingsLock::acquire(dir, deadline)?;
    let mut document = read_existing(path)?;
    set_preferences(&mut document, preferences);
    write_settings(path, document)
}

/// Writes `document` as the settings file: pretty JSON, a newline, the
/// profile's own staged rename. The one serializer every preference write
/// ends in.
fn write_settings(path: &Path, document: Map<String, Value>) -> io::Result<()> {
    let mut body = serde_json::to_string_pretty(&Value::Object(document))
        .expect("a settings object is always serializable");
    body.push('\n');
    write_document(path, body.as_bytes())
}

/// Applies a `/allowlist` change to the settings file at `path` and writes it
/// back, as one transaction under the same two locks [`write_preferences`]
/// takes (`applyPermissionPatch`, `settings_store.zig:430-436`).
///
/// `workspace` names the table: `None` for the profile's own, `Some(key)` for
/// that workspace's entry. **Nothing is written when nothing changed** -- a
/// rule already there, a rule not there to remove -- so a no-op neither moves
/// the file's mtime nor creates a profile. A file xfx cannot parse, or a table
/// that is not an object, is an error and the file is left as it was.
pub fn write_permission(
    path: &Path,
    workspace: Option<&str>,
    patch: &crate::permission::allowlist::Patch,
) -> io::Result<crate::permission::allowlist::Applied> {
    let _in_process = WRITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = parent_of(path)?;
    let mut document = read_existing(path)?;
    let applied = crate::permission::allowlist::apply(&mut document, workspace, patch)
        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))?;
    if !applied.changed {
        return Ok(applied);
    }
    create_private_dir(dir)?;
    let _across_processes = SettingsLock::acquire(dir, SETTINGS_LOCK_DEADLINE)?;
    // Read again under the lock: another process may have written between
    // the first read -- which decided whether there was anything to do -- and
    // the lock, and its change must survive this one.
    let mut document = read_existing(path)?;
    let applied = crate::permission::allowlist::apply(&mut document, workspace, patch)
        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))?;
    if applied.changed {
        write_settings(path, document)?;
    }
    Ok(applied)
}

/// Sets each preference's key path in `document`.
fn set_preferences(document: &mut Map<String, Value>, preferences: &[(&[&str], Value)]) {
    for (key_path, value) in preferences {
        let (last, parents) = key_path
            .split_last()
            .expect("every key path was checked above");
        let mut object = &mut *document;
        for key in parents {
            let slot = object
                .entry((*key).to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !slot.is_object() {
                *slot = Value::Object(Map::new());
            }
            object = slot
                .as_object_mut()
                .expect("the slot was made an object above");
        }
        object.insert((*last).to_string(), value.clone());
    }
}

/// One read, change and rename of the settings document, under the same two
/// locks [`write_preferences`] takes, for a change that is not "set these
/// keys".
///
/// `/workspace` is the one that needs it: it **removes** keys as well as
/// setting them, and an entry it empties has to leave the file rather than
/// stay behind as `{}` (`removeWorkspaceIfEmpty`, `vercel-labs/fx@c1db9191
/// src/core/config/settings_store.zig:1498-1505`). A second writer beside this
/// one would be a second place a concurrent key could be lost.
///
/// `edit` says whether it changed the document, and **nothing is written when
/// it did not**: the file stays byte-identical, which is what lets a caller
/// report `saved_changed=false` and mean it. An `Err` from `edit` is returned
/// as it is, with nothing written.
///
/// The document keeps its key order (`serde_json`'s `preserve_order`), so an
/// `edit` that removes a key removes it with `shift_remove`: `remove` would
/// move the last key into the hole.
pub fn edit_document<E: From<io::Error>>(
    path: &Path,
    edit: impl FnOnce(&mut Map<String, Value>) -> Result<bool, E>,
) -> Result<bool, E> {
    let _in_process = WRITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = parent_of(path)?;
    create_private_dir(dir)?;
    let _across_processes = SettingsLock::acquire(dir, SETTINGS_LOCK_DEADLINE)?;
    let mut document = read_existing(path)?;
    if !edit(&mut document)? {
        return Ok(false);
    }
    write_settings(path, document)?;
    Ok(true)
}

/// One read-modify-write of the whole settings file, under the same two locks
/// as [`write_preferences`]: the file as it stands **now** is read and handed
/// to `decide`, and the bytes `decide` answers are what is written.
///
/// What `xfx setup` writes through (`crate::provider::setup::commit`): its
/// selection is decided before -- the daemon probed, the catalog read -- and
/// merged here into whatever another writer left meanwhile, so a key written
/// between the two is kept rather than written over. A file that cannot be
/// read is an error and `decide` is not asked.
pub fn rewrite_document(
    path: &Path,
    decide: impl FnOnce(Map<String, Value>) -> Vec<u8>,
) -> io::Result<()> {
    let _in_process = WRITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = parent_of(path)?;
    create_private_dir(dir)?;
    let _across_processes = SettingsLock::acquire(dir, SETTINGS_LOCK_DEADLINE)?;
    let document = decide(read_existing(path)?);
    write_document(path, &document)
}

/// How long a settings write waits for another process's `settings.lock`
/// before it gives up: upstream's two seconds (`settings_store.zig:19`).
const SETTINGS_LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_millis(2000);

/// How long a write sleeps between two looks at a held `settings.lock`
/// (`acquireTimedAdvisoryLockControlled`, `io.zig:872-896`: ten milliseconds).
#[cfg(unix)]
const SETTINGS_LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(10);

/// The name of the lock beside the settings file, upstream's
/// (`settings_store.zig:491`).
const SETTINGS_LOCK_FILE: &str = "settings.lock";

/// One preference writer at a time **in this process**.
///
/// The TUI has two: `/statusline` on the thread holding the terminal and
/// `/model` on the runtime thread, and each is a read, a change and a rename.
/// Interleaved, the later rename puts back a document read before the earlier
/// one landed and the earlier key is lost. The file lock below would serialize
/// them too -- `flock` is per open file description -- but a mutex makes the
/// second thread wait its turn instead of polling the file every ten
/// milliseconds.
static WRITERS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `settings.lock`, held for one read-modify-write of the settings file
/// against **other processes**: a second xfx, or upstream on a home they share
/// the shape of.
///
/// Upstream's discipline (`acquireTimedAdvisoryLock`, `io.zig:780-896`): a
/// private file beside the settings, never followed through a link, locked
/// exclusively and without blocking, retried every ten milliseconds until a
/// deadline, and a failure -- not a wait for ever -- after it. Released by the
/// kernel when the file closes, so a write that panics or a process that dies
/// cannot leave it held. It is advisory: it stops writers that ask, which is
/// every writer of this file this module knows of.
struct SettingsLock {
    #[cfg(unix)]
    _file: fs::File,
}

impl SettingsLock {
    #[cfg(unix)]
    fn acquire(dir: &Path, deadline: std::time::Duration) -> io::Result<Self> {
        use rustix::fs::{flock, FlockOperation};
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(PRIVATE_MODE)
            // A link planted where the lock goes is refused rather than
            // followed to a file somebody else chose.
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join(SETTINGS_LOCK_FILE))?;
        let started = std::time::Instant::now();
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Self { _file: file }),
                Err(rustix::io::Errno::WOULDBLOCK) if started.elapsed() < deadline => {
                    std::thread::sleep(SETTINGS_LOCK_RETRY.min(deadline));
                }
                Err(rustix::io::Errno::WOULDBLOCK) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "another process is writing the settings file (settings.lock is held)",
                    ));
                }
                Err(err) => return Err(io::Error::from(err)),
            }
        }
    }

    /// No `flock` to take: the in-process mutex is all there is.
    #[cfg(not(unix))]
    fn acquire(dir: &Path, deadline: std::time::Duration) -> io::Result<Self> {
        let _ = (dir, deadline);
        Ok(Self {})
    }
}

/// What a preference that was written says when a layer above the profile
/// will still decide it for a fresh session, or nothing when none will.
///
/// Upstream's `reportUserSettingsCommit` with its caller's announcement
/// already made (`session_commands.zig:27-85`): the benign `saved to user
/// settings (scope=user)` is dropped, and what is kept is the suffix
/// `appendShadowedUserSource` adds for a field whose resolved source is not
/// the user's own file (`:225-236`) -- `fresh sessions here use
/// higher-precedence <field>=<source>`. `source` is where the value a fresh
/// session would be given comes from, as the loader ranked it at launch: an
/// environment override or this workspace's entry outranks what was just
/// written.
pub fn saved_but_shadowed(field: &str, source: crate::config::SettingSource) -> Option<String> {
    (source > crate::config::SettingSource::UserGlobal).then(|| {
        format!(
            "saved to user settings (scope=user); fresh sessions here use \
             higher-precedence {field}={}",
            source.label()
        )
    })
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
/// The read-modify-write around it is serialized by its callers' locks
/// ([`write_preferences`], [`rewrite_document`]); a caller that writes without
/// them is last-writer-wins.
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
    fn a_preference_is_one_nested_key_and_the_rest_of_the_file_survives() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profile").join("settings.json");

        // No file, and no profile home either: both are created, and the file
        // holds the one key and nothing else (upstream's partial merge,
        // `cap/fx-statusline-40x120-files.files`).
        write_preference(&path, &["statusLine", "session"], json!(true)).expect("write");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "{\n  \"statusLine\": {\n    \"session\": true\n  }\n}\n"
        );

        // A sibling under the same object, a key this build does not know and
        // a key at the top are all kept.
        fs::write(
            &path,
            "{\"permission_mode\":\"auto\",\"statusLine\":{\"session\":true,\"sandbox\":false}}",
        )
        .expect("seed");
        write_preference(&path, &["statusLine", "context"], json!(true)).expect("write");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            json!({
                "permission_mode": "auto",
                "statusLine": {"session": true, "sandbox": false, "context": true}
            })
        );

        // The same key again replaces its value rather than adding a second.
        write_preference(&path, &["statusLine", "session"], json!(false)).expect("write");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(written["statusLine"]["session"], json!(false));
        assert_eq!(written["statusLine"]["context"], json!(true));
    }

    #[test]
    fn a_preference_write_keeps_every_key_in_the_order_the_file_had_it() {
        // A permission table is read last-match-wins in document order
        // (`crate::permission::rules::RuleSet::decide`), so a write of any
        // other key that sorted the file would change what the table allows:
        // `{"rm*":"allow","*":"deny"}` denies everything, and sorted it would
        // allow `rm`. Upstream's writer keeps the order (`writeJsonValue` over
        // an insertion-ordered map, `settings_store.zig:1830-1850`).
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            "{\"zeta\":1,\"permission\":{\"bash\":{\"rm*\":\"allow\",\"*\":\"deny\"},\"edit\":\"ask\"},\"alpha\":2}",
        )
        .expect("seed");
        write_preference(&path, &["statusLine", "session"], json!(true)).expect("write");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "{\n  \"zeta\": 1,\n  \"permission\": {\n    \"bash\": {\n      \"rm*\": \"allow\",\n      \
             \"*\": \"deny\"\n    },\n    \"edit\": \"ask\"\n  },\n  \"alpha\": 2,\n  \
             \"statusLine\": {\n    \"session\": true\n  }\n}\n"
        );
    }

    #[test]
    fn a_value_in_the_way_of_a_preference_is_replaced_by_an_object() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, "{\"statusLine\":7,\"model\":\"m\"}\n").expect("seed");
        write_preference(&path, &["statusLine", "workspace"], json!(true)).expect("write");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            json!({"statusLine": {"workspace": true}, "model": "m"})
        );
    }

    #[test]
    fn a_preference_is_never_written_over_a_file_xfx_cannot_read() {
        // "xfx could not parse this" is not "this is empty": the operator's file
        // stays exactly as it was, and the write says why it did not happen.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        for original in ["{not json", "[1, 2]"] {
            fs::write(&path, original).expect("seed");
            let err = write_preference(&path, &["statusLine", "context"], json!(true))
                .expect_err("an unreadable file is refused");
            assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
            assert_eq!(fs::read_to_string(&path).expect("read"), original);
        }
        // And a preference with no key names nothing to write.
        let err = write_preference(&path, &[], json!(true)).expect_err("no key");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn a_preference_written_into_a_new_profile_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        write_preference(&path, &["statusLine", "context"], json!(true)).expect("write");
        assert_eq!(
            fs::metadata(&path).expect("stat").permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn two_writers_of_different_keys_at_once_both_survive() {
        // The TUI has two settings writers on two threads -- `/statusline` on
        // the one holding the terminal, `/model` on the runtime -- and each is
        // a read, a change and a rename. Unserialized, the second rename
        // replaces a document read before the first one landed, and the first
        // key is gone. Many rounds, because a lost update is a race and one
        // round can be lucky.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profile").join("settings.json");
        let writers: Vec<_> = (0..8)
            .map(|writer| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for round in 0..25 {
                        let key = format!("writer{writer}");
                        write_preference(&path, &[&key, &format!("round{round}")], json!(round))
                            .expect("write");
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().expect("a writer panicked");
        }
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        for writer in 0..8 {
            for round in 0..25 {
                assert_eq!(
                    written[format!("writer{writer}")][format!("round{round}")],
                    json!(round),
                    "writer {writer} lost round {round}: {written}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_write_waits_for_another_process_holding_the_settings_lock() {
        // Upstream's `settings.lock` beside the file (`settings_store.zig:491`,
        // `acquireTimedAdvisoryLock`): what keeps a second xfx -- or upstream
        // itself, on a shared home -- from writing between this one's read and
        // its rename. `flock` is per open file description, so a second open in
        // this process stands in for another process exactly.
        use rustix::fs::{flock, FlockOperation};
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}\n").expect("seed");
        let held = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.path().join("settings.lock"))
            .expect("open the lock");
        flock(&held, FlockOperation::NonBlockingLockExclusive).expect("hold the lock");

        let writer = {
            let path = path.clone();
            std::thread::spawn(move || write_preference(&path, &["effort"], json!("low")))
        };
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "{}\n",
            "the write went through a lock another process holds"
        );
        drop(held);
        writer.join().expect("the writer panicked").expect("write");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(written, json!({"effort": "low"}));
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_held_past_the_deadline_is_a_failure_rather_than_a_wait_for_ever() {
        use rustix::fs::{flock, FlockOperation};
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let held = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.path().join("settings.lock"))
            .expect("open the lock");
        flock(&held, FlockOperation::NonBlockingLockExclusive).expect("hold the lock");
        let err = write_preferences_within(
            &path,
            &[(&["effort"], json!("low"))],
            std::time::Duration::from_millis(50),
        )
        .expect_err("a held lock is not waited out for ever");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock, "{err}");
        assert!(!path.exists(), "the write happened anyway");
    }

    #[test]
    fn several_preferences_are_one_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, "{\"theme\":\"dark\"}").expect("seed");
        write_preferences(
            &path,
            &[
                (&["models", "gateway"], json!("openai/gpt-6-astra")),
                (&["effort"], json!("low")),
            ],
        )
        .expect("write");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            json!({"theme": "dark", "models": {"gateway": "openai/gpt-6-astra"}, "effort": "low"})
        );
        let err = write_preferences(&path, &[(&[], json!(true))]).expect_err("no key");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn a_save_is_said_only_when_a_layer_above_the_profile_still_decides() {
        use crate::config::SettingSource;
        for source in [
            SettingSource::CompiledDefault,
            SettingSource::Project,
            SettingSource::UserGlobal,
        ] {
            assert_eq!(
                saved_but_shadowed("permission_mode", source),
                None,
                "{source:?}"
            );
        }
        assert_eq!(
            saved_but_shadowed("permission_mode", SettingSource::ProcessOverride).as_deref(),
            Some(
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence permission_mode=process_override"
            )
        );
        assert_eq!(
            saved_but_shadowed("model", SettingSource::UserWorkspace).as_deref(),
            Some(
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence model=user_workspace"
            )
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

    #[test]
    fn a_preference_round_trips_through_the_configuration_and_keeps_every_other_key() {
        // Written into a temp HOME's profile and read back by the one thing
        // that decides what a settings file means, rather than by this module.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let path = home.path().join(".xfx").join("settings.json");
        fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        // `models` holds a scalar where an object belongs: replaced, not
        // merged into and not refused.
        fs::write(&path, r#"{"permission_mode":"ask","models":"stray"}"#).expect("seed");

        write_preference(&path, &["provider"], json!("llmux")).expect("write provider");
        write_preference(&path, &["models", "llmux"], json!("fable")).expect("write model");
        write_preference(&path, &["llmux_url"], json!("http://127.0.0.1:3456")).expect("url");

        let written = fs::read_to_string(&path).expect("read");
        assert!(
            written.ends_with("}\n"),
            "pretty JSON and a newline: {written:?}"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&written).expect("json"),
            json!({
                "permission_mode": "ask",
                "models": {"llmux": "fable"},
                "provider": "llmux",
                "llmux_url": "http://127.0.0.1:3456"
            })
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).expect("stat").permissions().mode() & 0o777,
                0o600
            );
        }

        let config = crate::config::RuntimeConfig::load_with(
            &crate::config::Environment::new(Some(home.path().to_path_buf()), Default::default()),
            workspace.path(),
        )
        .expect("the written profile loads");
        assert_eq!(config.provider, ProviderId::Llmux);
        assert_eq!(config.model, "fable");
        assert_eq!(
            config.permission_mode,
            crate::config::PermissionMode::Ask,
            "a key the write did not name was lost"
        );
    }

    #[test]
    fn a_preference_is_never_written_over_a_file_that_could_not_be_read() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").expect("seed");
        assert!(write_preference(&path, &["provider"], json!("llmux")).is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{ not json");
        // And a path of no keys names nothing to set.
        let empty = dir.path().join("empty.json");
        assert_eq!(
            write_preference(&empty, &[], json!(1))
                .expect_err("no key")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!empty.exists(), "a refused write left a file behind");
    }
}
