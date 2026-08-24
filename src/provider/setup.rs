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

/// Runs the provider setup command, dispatching to provider-specific logic.
pub async fn run(
    config: &RuntimeConfig,
    env: &Environment,
    provider: ProviderId,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError> {
    match provider {
        ProviderId::Gateway => run_gateway(config).await,
        ProviderId::Llmux => run_llmux(config, env, explicit_url).await,
    }
}

/// Set up the Vercel AI Gateway as the provider.
///
/// The gateway performs no I/O: it has no daemon to probe and advertises no
/// catalog. It resolves the credential, keeps the profile's model for that
/// provider, and publishes.
async fn run_gateway(config: &RuntimeConfig) -> Result<SetupReport, SetupError> {
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

    crate::provider::profile::write(&settings_path, existing, &selection).map_err(|e| {
        SetupError::Write {
            provider: ProviderId::Gateway,
            path: settings_path.clone(),
            detail: e.to_string(),
        }
    })?;

    // Step 6: Report what outranks the file and what is missing.
    let overridden_by = overriding_layers(config);

    let credential_source = credential.map(|cred| match cred {
        ProviderCredential::Bearer(bearer) => {
            CredentialSource::EnvVar(bearer.source_label().to_string())
        }
        ProviderCredential::KeylessLoopback => CredentialSource::KeylessLoopback,
    });

    Ok(SetupReport {
        provider: ProviderId::Gateway,
        url: None,
        models: None,
        model,
        model_reason,
        credential: credential_source,
        settings_path,
        overridden_by,
        credential_warning,
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
async fn run_llmux(
    config: &RuntimeConfig,
    env: &Environment,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError> {
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

    // Steps 3-6: Use the existing llmux setup logic.
    crate::llmux::setup::run(config, env, explicit_url).await
}
