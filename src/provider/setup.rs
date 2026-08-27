//! Provider switch as a transaction, with shared setup types.
//!
//! The types `SetupReport` and `SetupError` are defined here and used by all
//! providers. The actual discovery and setup logic is provider-specific:
//! `llmux::setup::run()` handles daemon discovery, while gateway setup is inline.

use std::path::PathBuf;

use crate::config::{Environment, RuntimeConfig, SettingSource};
use crate::gateway::EndpointError;
use crate::provider::profile::Selection;
use crate::provider::{authorizes, resolve_credential_for, ProviderCredential, ProviderId};

/// What `setup` decided, once it has decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupReport {
    /// The provider that was set up.
    pub provider: ProviderId,
    /// The daemon URL, if the provider has one. None for gateway.
    pub url: Option<String>,
    /// How many models the daemon offers, if it advertises a catalog.
    pub models: Option<usize>,
    /// The model that was recorded.
    pub model: String,
    /// Why that model, in one line.
    pub model_reason: String,
    /// The credential source that will be used.
    pub credential: Option<CredentialSource>,
    /// The settings file that was written.
    pub settings_path: PathBuf,
    /// What will still outrank the file setup just wrote.
    pub overridden_by: Option<String>,
    /// A warning about missing credential or configuration.
    pub credential_warning: Option<String>,
}

/// The source of a credential resolved for a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    /// Named environment variable holding the credential.
    EnvVar(String),
    /// Keyless loopback: llmux accepts requests on 127.0.0.1 with no credential.
    KeylessLoopback,
}

impl CredentialSource {
    pub fn label(&self) -> String {
        match self {
            Self::EnvVar(name) => name.clone(),
            Self::KeylessLoopback => crate::provider::LLMUX_LOOPBACK_LABEL.to_string(),
        }
    }
}

/// Why `setup` could not finish.
#[derive(Debug)]
pub enum SetupError {
    /// The `--url` the invocation named is not one xfx may send a prompt to.
    Endpoint(EndpointError),
    /// No daemon answered, and nothing named another place to look.
    NotFound {
        tried: Vec<String>,
        /// What the most informative candidate actually said, when one answered
        /// and was not llmux. "No daemon answered" is true and useless when the
        /// operator has a port conflict: it sends them to start something that
        /// is already running.
        answered: Option<String>,
    },
    /// Something answered, but it is not llmux.
    NotLlmux { url: String, detail: String },
    /// xfx could not build its own HTTP client, so nothing was contacted.
    ///
    /// Its own variant because reporting it as a daemon that did not answer
    /// blamed a daemon that was never asked -- and rendered as
    /// ``did not answer as an llmux daemon`` with an empty URL.
    Client { detail: String },
    /// There is no home directory, so there is no profile to write.
    NoProfile { provider: ProviderId },
    /// The existing settings file could not be read, so it must not be replaced.
    UnreadableSettings {
        provider: ProviderId,
        path: PathBuf,
        detail: String,
    },
    /// The settings file could not be written. The error message is provider-neutral.
    Write {
        provider: ProviderId,
        path: PathBuf,
        detail: String,
    },
    /// The credential source does not authorize this provider.
    Unauthorized {
        provider: ProviderId,
        source: String,
    },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Endpoint(err) => write!(f, "{err}"),
            Self::NotFound { tried, answered } => {
                write!(
                    f,
                    "no llmux daemon answered. xfx tried {}",
                    tried.join(", ")
                )?;
                if let Some(answered) = answered {
                    write!(f, ". {answered}")?;
                }
                write!(
                    f,
                    ". Start llmux, or name it with \
                     `xfx setup llmux --url http://127.0.0.1:<port>`"
                )
            }
            Self::NotLlmux { url, detail } => {
                write!(f, "`{url}` did not answer as an llmux daemon: {detail}")
            }
            Self::Client { detail } => {
                write!(f, "xfx could not build its HTTP client: {detail}")
            }
            Self::NoProfile { provider } => match provider {
                ProviderId::Llmux => write!(
                    f,
                    "xfx cannot record the daemon because no home directory is set, \
                         so there is no `~/.xfx/settings.json` to write"
                ),
                ProviderId::Gateway => write!(
                    f,
                    "xfx cannot configure gateway because no home directory is set, \
                         so there is no `~/.xfx/settings.json` to write"
                ),
            },
            Self::UnreadableSettings {
                provider,
                path,
                detail,
            } => {
                let provider_label = provider.label();
                write!(
                    f,
                    "xfx will not replace {}: {detail}. Fix or move the file and run \
                     `xfx setup {provider_label}` again -- xfx does not overwrite settings it could not read",
                    path.display()
                )
            }
            Self::Write {
                provider: _,
                path,
                detail,
            } => {
                write!(f, "cannot write {}: {detail}", path.display())
            }
            Self::Unauthorized { provider, source } => {
                write!(
                    f,
                    "provider `{}` does not accept credentials from {source}",
                    provider.label()
                )
            }
        }
    }
}

impl std::error::Error for SetupError {}

/// A setup that has decided everything and written nothing.
///
/// The seam this whole task turns on. `run` used to discover, probe, choose and
/// write in one call, so a caller could not know what the write would put on
/// disk without performing it -- and a caller that must be able to *undo* the
/// write, or to abandon it while it is still abandonable, has to know exactly
/// that. Every network and read step happens before this value exists; nothing
/// after it does any I/O except [`commit`], which writes `document` and nothing
/// else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSetup {
    /// What the operator will be told, once this is committed.
    pub report: SetupReport,
    /// The file `document` belongs in.
    pub settings_path: PathBuf,
    /// The exact bytes, from `profile::document_for` -- the same serializer the
    /// pre-seam `profile::write` used, so `xfx setup` still produces the same
    /// file byte for byte.
    pub document: Vec<u8>,
}

/// Runs the provider setup command: decide everything, then write it.
///
/// **Defined as `prepare` then `commit`** rather than reimplemented beside
/// them, so the CLI path and the TUI's transaction cannot drift: there is one
/// decision procedure and one writer, and `xfx setup` keeps its public behaviour
/// and its on-disk bytes because it is now literally the same two calls the
/// transaction makes.
pub async fn run(
    config: &RuntimeConfig,
    env: &Environment,
    provider: ProviderId,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError> {
    let prepared = prepare(config, env, provider, explicit_url).await?;
    commit(&prepared)?;
    Ok(prepared.report)
}

/// Everything `run` decides, with nothing written.
///
/// Performs the discovery, the catalog probe, the model selection and the merge,
/// and returns the document those produced. **Touches the settings file only to
/// read it**, so a caller may drop the result and leave the machine exactly as
/// it found it -- which is what makes a cancellation window possible at all.
pub async fn prepare(
    config: &RuntimeConfig,
    env: &Environment,
    provider: ProviderId,
    explicit_url: Option<&str>,
) -> Result<PreparedSetup, SetupError> {
    match provider {
        ProviderId::Gateway => prepare_gateway(config).await,
        ProviderId::Llmux => prepare_llmux(config, env, explicit_url).await,
    }
}

/// Writes exactly [`PreparedSetup::document`], and nothing else.
///
/// The only I/O this module performs after [`prepare`] has returned. It does not
/// re-read, re-merge or re-decide: a commit that recomputed the document could
/// write something the caller never saw and could therefore never recognize
/// afterwards -- and recognizing its own bytes on disk is exactly how the
/// caller tells "the rename landed" from "another writer owns this file".
pub fn commit(prepared: &PreparedSetup) -> Result<(), SetupError> {
    crate::provider::profile::write_document(&prepared.settings_path, &prepared.document).map_err(
        |err| SetupError::Write {
            provider: prepared.report.provider,
            path: prepared.settings_path.clone(),
            detail: err.to_string(),
        },
    )
}

/// Set up the Vercel AI Gateway as the provider.
///
/// The gateway performs no I/O: it has no daemon to probe and advertises no
/// catalog. It resolves the credential, keeps the profile's model for that
/// provider, and publishes.
async fn prepare_gateway(config: &RuntimeConfig) -> Result<PreparedSetup, SetupError> {
    // Get the settings path
    let settings_path = config
        .user_settings_path
        .clone()
        .ok_or(SetupError::NoProfile {
            provider: ProviderId::Gateway,
        })?;

    // Step 1: Resolve the gateway's credential.
    let credential = resolve_credential_for(ProviderId::Gateway, config);

    // Step 2: Check that the credential authorizes the provider when present.
    if let Some(cred) = &credential {
        let source = cred.source();
        if !authorizes(ProviderId::Gateway, source) {
            return Err(SetupError::Unauthorized {
                provider: ProviderId::Gateway,
                source: format!("{:?}", source),
            });
        }
    }

    let credential_warning = if credential.is_none() {
        Some(crate::output::MISSING_AUTH_HELP.to_string())
    } else {
        None
    };

    // Steps 3-4: Gateway has no catalog, so pick the model from configuration or default.
    let model = config
        .models
        .get("gateway")
        .cloned()
        .unwrap_or_else(|| crate::config::DEFAULT_MODEL.to_string());
    let model_reason = if config.models.contains_key("gateway") {
        "profile"
    } else {
        "compiled_default"
    }
    .to_string();

    // Step 5: Write the profile with the gateway selected.
    let existing = crate::provider::profile::read_existing(&settings_path).map_err(|e| {
        SetupError::UnreadableSettings {
            provider: ProviderId::Gateway,
            path: settings_path.clone(),
            detail: e.to_string(),
        }
    })?;

    let selection = Selection {
        provider: ProviderId::Gateway,
        model: &model,
        llmux_url: None,
    };

    // The document, not the write. Nothing has touched the file except the read
    // above, so a caller that drops this leaves the machine untouched.
    let document = crate::provider::profile::document_for(existing, &selection);

    // Step 6: Report what outranks the file and what is missing.
    let overridden_by = overriding_layers(config);

    let credential_source = credential.map(|cred| match cred {
        ProviderCredential::Bearer(bearer) => {
            CredentialSource::EnvVar(bearer.source_label().to_string())
        }
        ProviderCredential::KeylessLoopback => CredentialSource::KeylessLoopback,
    });

    Ok(PreparedSetup {
        report: SetupReport {
            provider: ProviderId::Gateway,
            url: None,
            models: None,
            model,
            model_reason,
            credential: credential_source,
            settings_path: settings_path.clone(),
            overridden_by,
            credential_warning,
        },
        settings_path,
        document,
    })
}

/// What will still outrank the profile once setup has written it.
fn overriding_layers(config: &RuntimeConfig) -> Option<String> {
    let mut layers: Vec<String> = Vec::new();
    if config.sources.model == SettingSource::ProcessOverride {
        layers.push(crate::config::ENV_MODEL.to_string());
    }
    if config.sources.model == SettingSource::UserWorkspace
        || config.sources.provider == SettingSource::UserWorkspace
    {
        layers.push(format!(
            "the workspaces entry for {}",
            config.workspace_root.display()
        ));
    }
    (!layers.is_empty()).then(|| layers.join(", "))
}

/// Set up a local llmux daemon as the provider.
///
/// Delegates to the existing llmux setup logic and wraps the result.
async fn prepare_llmux(
    config: &RuntimeConfig,
    env: &Environment,
    explicit_url: Option<&str>,
) -> Result<PreparedSetup, SetupError> {
    // Step 1: Resolve llmux credential (keyless loopback when url is configured).
    let credential = resolve_credential_for(ProviderId::Llmux, config);

    // Step 2: Check that credential authorizes llmux (only KeylessLoopback is valid).
    if let Some(cred) = &credential {
        let source = cred.source();
        if !authorizes(ProviderId::Llmux, source) {
            return Err(SetupError::Unauthorized {
                provider: ProviderId::Llmux,
                source: format!("{:?}", source),
            });
        }
    }

    // Steps 3-6: Use the existing llmux setup logic, up to but not including
    // the write.
    crate::llmux::setup::prepare(config, env, explicit_url).await
}

/// Why a provider switch did not end in a swap.
///
/// Three outcomes rather than one error, because the *runtime* has to do three
/// different things with them and the difference is not a matter of wording.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SetupProblem {
    /// Reported to the user; the session continues on the provider it had.
    Failed(String),
    /// The cancel window won. Nothing was written, so there is nothing to say
    /// beyond the turn being over.
    Cancelled,
    /// The settings file holds bytes this transaction neither wrote nor found.
    /// **Another writer owns it.** Nothing may be written over that, and this
    /// process must not go on pretending it knows what its configuration is.
    Conflict(String),
}

/// What the settings file holds now, relative to this transaction.
///
/// Read from the **raw bytes**, never from the error value a write returned.
/// The reason is the one the whole transaction is built around: a failed
/// `write_document` may have failed before the rename -- in which case the
/// operator's file is untouched -- or after it, on the directory sync, in which
/// case the new document is already in place and the only thing that failed was
/// its durability (`crate::provider::profile`'s `replace_private_file_at`). Those
/// are opposite facts about the machine and the `io::Error` cannot tell them
/// apart, so it is not asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiskState {
    /// Exactly the bytes this transaction prepared: the rename landed.
    Document,
    /// Exactly the preimage: the rename did not.
    Preimage,
    /// Neither. Somebody else wrote this file while the transaction was running.
    Third,
}

/// Classifies the settings file by rereading it.
///
/// `Document` is tested first on purpose. A setup that changes nothing produces
/// a document equal to the preimage, and calling that "committed" is both true
/// and the safe direction: the file already says what was wanted, and a rollback
/// would be a write nobody needs.
fn classify(
    path: &std::path::Path,
    document: &[u8],
    preimage: &crate::provider::profile::ProfilePreimage,
) -> DiskState {
    let current = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        // Unreadable is not "absent" and is certainly not "mine": a file this
        // process cannot read is a file it must not overwrite.
        Err(_) => return DiskState::Third,
    };
    match (current, preimage) {
        (Some(bytes), _) if bytes == document => DiskState::Document,
        (None, crate::provider::profile::ProfilePreimage::Missing) => DiskState::Preimage,
        (Some(bytes), crate::provider::profile::ProfilePreimage::Present { bytes: was, .. })
            if bytes == *was =>
        {
            DiskState::Preimage
        }
        _ => DiskState::Third,
    }
}

/// What the user is told when another writer owns the settings file.
const CONFLICT: &str = "xfx: another process wrote ~/.xfx/settings.json while this switch was \
     running, so xfx changed nothing and cannot say what the configuration now is";

/// The provider switch, as a transaction, in the order the plan fixes.
///
/// Seven steps, and each is here because of the one before it:
///
/// a. **Snapshot** the preimage before anything else touches disk, so a rollback
///    has something byte-exact to put back and can tell "there was no file" from
///    "there was an empty one".
/// b. **Prepare.** Every network and read step -- discovery, the catalog probe,
///    the model selection, the merge -- happens here and writes nothing.
/// c. **The cancel window**, and it is the whole of it: cancellation may win
///    only between (b) and (d). Once the commit has started the user's Ctrl-C
///    cannot un-write a file, and a transaction that pretended otherwise would
///    leave the session running against a provider the disk disagrees with.
/// d. **Commit** exactly the prepared bytes.
/// e. **Classify a commit error by rereading raw disk bytes.** Never by trusting
///    the error. Equal to the document = the rename landed and only the
///    directory sync failed: committed, carry on. Equal to the preimage = not
///    committed: keep the old runtime and report. Anything else = another writer
///    owns the file: **do not overwrite it.**
/// f. **Reload**, and on a reload failure reread once more and roll back *only
///    if* the disk still holds this transaction's own document. A third state
///    here is not this transaction's to undo either.
/// g. **Swap** -- the caller's, and only after a reload that succeeded.
///
/// The only serialization claimed is the one actually enforced: `WORK_LIMIT` is
/// two, so a second `Setup` can be *accepted*, but [`turn_loop`] takes work one
/// item at a time on a single thread, so only one transaction ever *executes*.
/// There is no cross-process lock -- file replacement is atomic, the
/// read-modify-write around it is last-writer-wins by its own documented
/// contract (`crate::provider::profile`) -- which is exactly why (e) and (f)
/// classify by rereading and why a third state is never overwritten.
///
/// `cancelled` and `reload` are arguments rather than statements so that every
/// branch above is a claim a unit test can make without a daemon, a terminal or
/// a second process.
pub(crate) async fn setup_transaction(
    config: &RuntimeConfig,
    env: &Environment,
    provider: ProviderId,
    cancelled: &dyn Fn() -> bool,
    reload: &dyn Fn() -> Result<RuntimeConfig, String>,
) -> Result<RuntimeConfig, SetupProblem> {
    let settings_path = config
        .user_settings_path
        .clone()
        .ok_or_else(|| SetupProblem::Failed(no_profile(provider)))?;

    // (a) Snapshot.
    let preimage = crate::provider::profile::snapshot(&settings_path).map_err(|err| {
        SetupProblem::Failed(format!(
            "xfx: cannot read {} to switch provider: {err}",
            settings_path.display()
        ))
    })?;

    // (b) Prepare. Nothing written.
    let prepared = prepare(config, env, provider, None)
        .await
        .map_err(|err| SetupProblem::Failed(format!("xfx: {err}")))?;

    // (c) The cancel window closes here.
    if cancelled() {
        return Err(SetupProblem::Cancelled);
    }

    // (d) Commit, and (e) classify a failure by rereading.
    if let Err(err) = commit(&prepared) {
        match classify(&settings_path, &prepared.document, &preimage) {
            // The rename landed; the directory sync did not. The document is on
            // disk, so the switch happened -- it is merely less durable than it
            // should be, and reporting it as a failure would leave the session
            // on a provider the file no longer names.
            DiskState::Document => {}
            DiskState::Preimage => {
                return Err(SetupProblem::Failed(format!("xfx: {err}")));
            }
            DiskState::Third => return Err(SetupProblem::Conflict(CONFLICT.to_string())),
        }
    }

    // (f) Reload.
    match reload() {
        Ok(reloaded) => Ok(reloaded),
        Err(reason) => match classify(&settings_path, &prepared.document, &preimage) {
            DiskState::Document => {
                // Ours, still. Undo it exactly -- bytes and mode, or the file's
                // absence -- and say why the session did not move.
                let rolled_back = crate::provider::profile::restore(&settings_path, &preimage);
                Err(SetupProblem::Failed(match rolled_back {
                    Ok(()) => format!(
                        "xfx: {} was written but could not be re-read ({reason}), \
                         so it was put back as it was",
                        settings_path.display()
                    ),
                    Err(err) => format!(
                        "xfx: {} was written, could not be re-read ({reason}), \
                         and could not be put back: {err}",
                        settings_path.display()
                    ),
                }))
            }
            // Never committed, so there is nothing to undo.
            DiskState::Preimage => Err(SetupProblem::Failed(format!(
                "xfx: the configuration could not be re-read: {reason}"
            ))),
            DiskState::Third => Err(SetupProblem::Conflict(CONFLICT.to_string())),
        },
    }
}

/// Why there is no profile to write for `provider`.
fn no_profile(provider: ProviderId) -> String {
    format!(
        "xfx: cannot set up {} because no home directory is set, so there is no \
         `~/.xfx/settings.json` to write",
        provider.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Environment;
    use crate::provider::profile::{self, ProfilePreimage};
    use std::collections::BTreeMap;
    use std::fs;

    /// A configuration whose profile lives in `home`, with nothing in the
    /// environment.
    ///
    /// Through `load_with` rather than by building the struct, because what is
    /// being exercised is a *round trip*: `prepare` reads the file the loader
    /// read, and the reload at the end of a transaction is the same loader
    /// again. A hand-made config would prove a merge, not a transaction.
    fn config_in(home: &std::path::Path, workspace: &std::path::Path) -> RuntimeConfig {
        RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), BTreeMap::new()),
            workspace,
        )
        .expect("load config")
    }

    fn env_in(home: &std::path::Path) -> Environment {
        Environment::new(Some(home.to_path_buf()), BTreeMap::new())
    }

    struct Fixture {
        _root: tempfile::TempDir,
        home: PathBuf,
        workspace: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("temp root");
            let home = root.path().join("home");
            let workspace = root.path().join("workspace");
            fs::create_dir_all(&home).expect("home");
            fs::create_dir_all(&workspace).expect("workspace");
            Self {
                _root: root,
                home,
                workspace,
            }
        }
        fn config(&self) -> RuntimeConfig {
            config_in(&self.home, &self.workspace)
        }
        fn env(&self) -> Environment {
            env_in(&self.home)
        }
        fn settings(&self) -> PathBuf {
            self.home.join(".xfx").join("settings.json")
        }
        fn reload(&self) -> impl Fn() -> Result<RuntimeConfig, String> + '_ {
            move || {
                RuntimeConfig::load_with(&self.env(), &self.workspace).map_err(|e| e.to_string())
            }
        }
    }

    // -----------------------------------------------------------------------
    // prepare and commit
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn commit_writes_byte_for_byte_what_prepare_returned() {
        // The property the whole transaction rests on. `prepare` hands out a
        // document and the caller then has to be able to recognize *those exact
        // bytes* on disk afterwards -- that recognition is how a failed write is
        // classified and how a rollback decides it is undoing its own work. A
        // commit that recomputed anything would break both.
        let fixture = Fixture::new();
        let prepared = prepare(&fixture.config(), &fixture.env(), ProviderId::Gateway, None)
            .await
            .expect("prepare");
        assert!(
            !fixture.settings().exists(),
            "prepare wrote something; it must decide everything and write nothing"
        );
        commit(&prepared).expect("commit");
        assert_eq!(
            fs::read(fixture.settings()).expect("read"),
            prepared.document,
            "the bytes on disk are not the bytes prepare promised"
        );
    }

    #[tokio::test]
    async fn cli_setup_still_writes_the_pre_seam_bytes_through_prepare_then_commit() {
        // `run` is now `prepare` then `commit`, and this is the claim that the
        // split changed nothing an operator can see: the file `xfx setup` leaves
        // behind is byte-identical to what the pre-seam `profile::write` -- the
        // merge-and-serialize this module used to perform inline -- produces from
        // the same inputs.
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.home.join(".xfx")).expect("profile dir");
        fs::write(
            fixture.settings(),
            b"{\n  \"permission_mode\": \"auto\",\n  \"models\": {\"llmux\": \"fable\"}\n}\n",
        )
        .expect("seed a profile with unrelated keys");

        let config = fixture.config();
        let expected = {
            let existing = profile::read_existing(&fixture.settings()).expect("read");
            let model = config
                .models
                .get("gateway")
                .cloned()
                .unwrap_or_else(|| crate::config::DEFAULT_MODEL.to_string());
            profile::document_for(
                existing,
                &Selection {
                    provider: ProviderId::Gateway,
                    model: &model,
                    llmux_url: None,
                },
            )
        };

        run(&config, &fixture.env(), ProviderId::Gateway, None)
            .await
            .expect("run");
        assert_eq!(fs::read(fixture.settings()).expect("read"), expected);
        // And the keys that were not this setup's business survived it.
        let after = profile::read_existing(&fixture.settings()).expect("read");
        assert_eq!(after["permission_mode"], "auto");
        assert_eq!(after["models"]["llmux"], "fable");
        assert_eq!(after["models"]["gateway"], crate::config::DEFAULT_MODEL);
    }

    // -----------------------------------------------------------------------
    // the transaction, branch by branch
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn a_cancellation_before_the_commit_leaves_the_machine_untouched() {
        // (c). The window is between prepare and commit and it is the whole of
        // it: nothing has been written yet, so "stop" can still mean "as if it
        // never happened".
        let fixture = Fixture::new();
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| true,
            &fixture.reload(),
        )
        .await
        .expect_err("a cancelled transaction does not swap");
        assert_eq!(problem, SetupProblem::Cancelled);
        assert!(
            !fixture.settings().exists(),
            "a cancelled transaction wrote a settings file"
        );
    }

    #[tokio::test]
    async fn a_cancellation_that_arrives_after_the_commit_is_ignored() {
        // The other half of (c), and the half that is easy to get wrong by
        // being generous: the window is between (b) and (d) and **nowhere
        // else**. Once the commit has started, "stop" cannot un-write a file --
        // so a transaction that kept listening would report `Cancelled` for a
        // switch that had already happened, and leave the session running
        // against a provider the disk disagrees with.
        //
        // Driven by a flag that is false the first time it is read and true
        // afterwards, which is exactly the interleaving a user produces by
        // pressing Ctrl-C while the write is in flight.
        let fixture = Fixture::new();
        let reads = std::cell::Cell::new(0usize);
        let cancelled = || {
            reads.set(reads.get() + 1);
            reads.get() > 1
        };
        let reloaded = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &cancelled,
            &fixture.reload(),
        )
        .await
        .expect("a cancellation after the commit does not undo it");

        assert_eq!(reloaded.provider, ProviderId::Gateway);
        assert_eq!(
            reads.get(),
            1,
            "the cancellation was read more than once: the window is (b)..(d) and nowhere else"
        );
        assert!(
            fixture.settings().exists(),
            "the committed document was removed by a cancellation that arrived too late"
        );
    }

    #[cfg(feature = "fault-injection")]
    #[tokio::test]
    async fn a_failure_before_the_rename_is_classified_as_not_committed() {
        // (e), first arm. The stage was written and the rename did not happen,
        // so the operator's file is exactly as it was -- and the transaction has
        // to say so by *reading the disk*, because the `io::Error` it was handed
        // cannot tell this case from the one below.
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.home.join(".xfx")).expect("profile dir");
        let before = b"{\n  \"permission_mode\": \"auto\"\n}\n";
        fs::write(fixture.settings(), before).expect("seed");

        profile::fault::arm(profile::fault::Boundary::Rename);
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &fixture.reload(),
        )
        .await
        .expect_err("a write that did not land does not swap");
        profile::fault::disarm();

        match problem {
            SetupProblem::Failed(message) => assert!(message.starts_with("xfx: "), "{message}"),
            other => panic!("classified as {other:?} rather than a plain failure"),
        }
        assert_eq!(
            fs::read(fixture.settings()).expect("read"),
            before,
            "the file was changed by a write that was supposed not to have landed"
        );
        // And no stage was left behind to be mistaken for state.
        let strays: Vec<_> = fs::read_dir(fixture.home.join(".xfx"))
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "settings.json")
            .collect();
        assert_eq!(strays, Vec::<String>::new(), "a stage survived the failure");
    }

    #[cfg(feature = "fault-injection")]
    #[tokio::test]
    async fn a_failure_on_the_directory_sync_is_classified_as_committed() {
        // (e), second arm, and the reason the classification reads bytes rather
        // than the error: the rename landed. The document *is* the file. Calling
        // this a failure would leave the session talking to a provider the
        // settings file no longer names.
        let fixture = Fixture::new();
        profile::fault::arm(profile::fault::Boundary::DirectorySync);
        let reloaded = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &fixture.reload(),
        )
        .await
        .expect("a landed rename is a committed transaction");
        profile::fault::disarm();

        assert_eq!(reloaded.provider, ProviderId::Gateway);
        let prepared = prepare(&fixture.config(), &fixture.env(), ProviderId::Gateway, None)
            .await
            .expect("prepare");
        assert_eq!(
            fs::read(fixture.settings()).expect("read"),
            prepared.document,
            "the committed document is not what is on disk"
        );
    }

    #[cfg(feature = "fault-injection")]
    #[tokio::test]
    async fn a_third_state_at_the_commit_is_never_overwritten() {
        // (e), third arm. Somebody else owns this file now. The transaction has
        // no claim on those bytes, so it reports and stops rather than writing
        // over a document it did not produce and cannot reconstruct.
        //
        // The cancel callback is the transaction's one observable point between
        // prepare and commit, so it is where a concurrent writer is staged: a
        // second real process would be the same interleaving with more moving
        // parts and less determinism.
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.home.join(".xfx")).expect("profile dir");
        fs::write(fixture.settings(), b"{}\n").expect("seed");
        let intruder = b"{\n  \"written\": \"by somebody else\"\n}\n";

        profile::fault::arm(profile::fault::Boundary::Rename);
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| {
                fs::write(fixture.settings(), intruder).expect("the other writer");
                false
            },
            &fixture.reload(),
        )
        .await
        .expect_err("a third state does not swap");
        profile::fault::disarm();

        assert!(matches!(problem, SetupProblem::Conflict(_)), "{problem:?}");
        assert_eq!(
            fs::read(fixture.settings()).expect("read"),
            intruder,
            "the other writer's document was not left byte-identical"
        );
    }

    #[tokio::test]
    async fn a_third_state_at_the_reload_is_never_overwritten_either() {
        // (f), third arm. The commit landed, the reload failed, and by the time
        // the rollback looked the file was somebody else's. A rollback is only
        // ever allowed to undo *its own* write.
        let fixture = Fixture::new();
        let intruder = b"{\n  \"written\": \"by somebody else\"\n}\n";
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &|| {
                fs::write(fixture.settings(), intruder).expect("the other writer");
                Err("the configuration could not be parsed".to_string())
            },
        )
        .await
        .expect_err("a third state does not swap");

        assert!(matches!(problem, SetupProblem::Conflict(_)), "{problem:?}");
        assert_eq!(fs::read(fixture.settings()).expect("read"), intruder);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_reload_failure_puts_a_present_preimage_back_exactly() {
        // (f), rollback with something to put back: the bytes **and** the mode.
        // A `0644` profile that came back `0600` would be this transaction
        // tightening a file it was only supposed to be undoing.
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.home.join(".xfx")).expect("profile dir");
        let before = b"{\n  \"permission_mode\": \"auto\"\n}\n";
        fs::write(fixture.settings(), before).expect("seed");
        fs::set_permissions(fixture.settings(), fs::Permissions::from_mode(0o644)).expect("chmod");

        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &|| Err("the configuration could not be parsed".to_string()),
        )
        .await
        .expect_err("a reload that failed does not swap");

        assert!(matches!(problem, SetupProblem::Failed(_)), "{problem:?}");
        assert_eq!(fs::read(fixture.settings()).expect("read"), before);
        assert_eq!(
            fs::metadata(fixture.settings())
                .expect("stat")
                .permissions()
                .mode()
                & 0o777,
            0o644,
            "the preimage's mode was not restored"
        );
    }

    #[tokio::test]
    async fn a_reload_failure_unlinks_a_file_this_transaction_created() {
        // (f), rollback with nothing to put back. `Missing` is not `{}`: leaving
        // an empty object behind would leave a profile the operator never had,
        // and every later setup would merge into it.
        let fixture = Fixture::new();
        assert!(!fixture.settings().exists());
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &|| Err("the configuration could not be parsed".to_string()),
        )
        .await
        .expect_err("a reload that failed does not swap");

        assert!(matches!(problem, SetupProblem::Failed(_)), "{problem:?}");
        assert!(
            !fixture.settings().exists(),
            "the file this transaction created was left behind"
        );
    }

    #[cfg(feature = "fault-injection")]
    #[tokio::test]
    async fn a_rollback_that_cannot_finish_says_so_rather_than_claiming_it_did() {
        // The parent-sync boundary of the `Missing` restore path. The file is
        // gone, the directory entry is not durable, and the operator is told the
        // second half rather than being given a message that says the machine is
        // back as it was.
        let fixture = Fixture::new();
        profile::fault::arm(profile::fault::Boundary::ParentSync);
        let problem = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &|| Err("the configuration could not be parsed".to_string()),
        )
        .await
        .expect_err("a reload that failed does not swap");
        profile::fault::disarm();

        match problem {
            SetupProblem::Failed(message) => assert!(
                message.contains("could not be put back"),
                "a rollback that failed reported success: {message}"
            ),
            other => panic!("{other:?}"),
        }
        assert!(!fixture.settings().exists(), "the unlink itself did happen");
    }

    #[tokio::test]
    async fn a_transaction_that_succeeded_reloads_rather_than_believing_the_report() {
        // (f) and (g)'s input. What the caller gets back is what `load_with`
        // made of the file, so a layer above the profile still wins -- which is
        // the one case where the report and the truth differ.
        let fixture = Fixture::new();
        let reloaded = setup_transaction(
            &fixture.config(),
            &fixture.env(),
            ProviderId::Gateway,
            &|| false,
            &fixture.reload(),
        )
        .await
        .expect("the transaction");
        assert_eq!(reloaded.provider, ProviderId::Gateway);
        assert!(reloaded.user_settings_loaded, "the file was re-read");
        assert_eq!(
            reloaded.model,
            crate::config::DEFAULT_MODEL,
            "the model came from the reloaded configuration"
        );
    }

    #[test]
    fn the_classification_reads_bytes_and_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let document = b"document\n".to_vec();

        // Absent, against a preimage that was absent: not committed.
        assert_eq!(
            classify(&path, &document, &ProfilePreimage::Missing),
            DiskState::Preimage
        );
        // Absent, against a preimage that was there: somebody removed it.
        assert_eq!(
            classify(
                &path,
                &document,
                &ProfilePreimage::Present {
                    bytes: b"was\n".to_vec(),
                    mode: 0o600
                }
            ),
            DiskState::Third
        );
        fs::write(&path, &document).expect("write");
        assert_eq!(
            classify(&path, &document, &ProfilePreimage::Missing),
            DiskState::Document
        );
        fs::write(&path, b"neither\n").expect("write");
        assert_eq!(
            classify(&path, &document, &ProfilePreimage::Missing),
            DiskState::Third
        );
        fs::write(&path, b"was\n").expect("write");
        assert_eq!(
            classify(
                &path,
                &document,
                &ProfilePreimage::Present {
                    bytes: b"was\n".to_vec(),
                    mode: 0o600
                }
            ),
            DiskState::Preimage
        );

        // A path this process cannot read is not "absent", and it is certainly
        // not "mine". Reading a directory fails with something that is not
        // `NotFound`, and the safe answer to *any* unreadable path is the one
        // that refuses to write: the whole point of the third state is that
        // bytes this transaction cannot account for are never overwritten, and
        // "I could not even look" is the least accountable state there is.
        let occupied = dir.path().join("occupied");
        fs::create_dir(&occupied).expect("a directory where a file belongs");
        assert_eq!(
            classify(&occupied, &document, &ProfilePreimage::Missing),
            DiskState::Third,
            "a path that could not be read was treated as absent"
        );
    }
}
