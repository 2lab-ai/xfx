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
    let merged = merge_selection(existing, selection, selection.provider.legacy_backend());

    let mut body = serde_json::to_string_pretty(&Value::Object(merged))
        .expect("a settings object is always serializable");
    body.push('\n');

    let dir = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the settings path has no parent directory",
        )
    })?;
    create_private_dir(dir)?;
    replace_private_file(dir, path, body.as_bytes())
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
/// one -- never a half-written file.
pub fn replace_private_file(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
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
            options.mode(0o600);
        }
        let mut file = options.open(&staged.path)?;
        io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    // A rename onto the target replaces its inode atomically. On failure the
    // guard removes this write's own stage on the way out -- and only that one.
    fs::rename(&staged.path, path)?;
    staged.commit();
    sync_directory(dir)
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
