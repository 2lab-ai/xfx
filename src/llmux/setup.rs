//! `xfx setup llmux`: find the local daemon, agree on a model, record it.
//!
//! The command exists because the alternative is telling an operator to hand-
//! write two settings keys and a model id they have no way to look up. It does
//! three things and refuses rather than guessing at any of them:
//!
//! 1. **Find a daemon.** An explicit `--url`, else the default loopback port,
//!    else the port named by llmux's own configuration file. Nothing else -- a
//!    scan would be a port scan, and a remote guess would be a prompt sent
//!    somewhere nobody named.
//! 2. **Prove it is llmux.** `GET /` must answer exactly `llmux` and
//!    `GET /models` must answer a non-empty catalog. Any HTTP server on loopback
//!    can answer 200; recording a URL that is something else would point every
//!    later turn at whatever happened to be listening on that port. The catalog
//!    is the second half of the proof, because it shows the data plane answers a
//!    keyless loopback request, which is the whole credential story.
//! 3. **Record it.** `backend`, `llmux_url` and `model` are merged into
//!    `~/.xfx/settings.json`, preserving every other key, and written through a
//!    staged file and a rename.
//!
//! Two things it deliberately does not do. It sends **no completion request**:
//! the root ping and the catalog are the receipt, and a setup command that spent
//! a token to prove a daemon was there would be a setup command an operator
//! learned to avoid running. And it never reads, copies or writes an llmux
//! credential -- llmux's configuration file holds OAuth tokens and admin keys
//! next to the port, and exactly one `u16` is read out of it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{
    Environment, RuntimeConfig, SettingSource, ENV_LLMUX_CONFIG, ENV_XDG_CONFIG_HOME,
    MAX_SETTINGS_BYTES,
};
use crate::provider::model;
use crate::provider::profile;

use super::DEFAULT_URL;

// Re-export the shared setup types from provider::setup
pub use crate::provider::setup::{SetupError, SetupReport};

/// The body `GET /` answers on a real daemon
/// (`2lab-ai/llmux@79f66748656b src/proxy/server.rs:1240`).
const ROOT_BODY: &str = "llmux";

/// llmux's configuration file name, under whichever directory holds it.
const LLMUX_CONFIG_FILE: &str = "llmux.json";

/// Runs the whole command: discover, probe, choose, record.
pub async fn run(
    config: &RuntimeConfig,
    env: &Environment,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError> {
    let (url, catalog) = discover(config, env, explicit_url).await?;
    let settings_path = config
        .user_settings_path
        .clone()
        .ok_or(SetupError::NoProfile {
            provider: crate::provider::ProviderId::Llmux,
        })?;

    // The file, read once, before anything is decided. The keep-or-replace
    // decision has to be about the layer being *written*: reading the fully
    // resolved `config.model` meant an `XFX_MODEL` in the shell got persisted
    // into the profile -- destroying the profile's own value -- and the write
    // was then a no-op for that shell, because the environment outranks it.
    // Reported, of course, as "kept".
    let existing =
        profile::read_existing(&settings_path).map_err(|err| SetupError::UnreadableSettings {
            provider: crate::provider::ProviderId::Llmux,
            path: settings_path.clone(),
            detail: err.to_string(),
        })?;

    // Configured model follows the same precedence as the loader: models[llmux]
    // when present, else the flat model, else the default.
    let configured = existing
        .get("models")
        .and_then(Value::as_object)
        .and_then(|models| models.get("llmux"))
        .and_then(Value::as_str)
        .or_else(|| existing.get("model").and_then(Value::as_str))
        .unwrap_or(crate::config::DEFAULT_MODEL);
    let (model, model_reason) = select_model(configured, &catalog);

    let selection = profile::Selection {
        provider: crate::provider::ProviderId::Llmux,
        model: &model,
        llmux_url: Some(&url),
    };
    profile::write(&settings_path, existing, &selection).map_err(|err| SetupError::Write {
        provider: crate::provider::ProviderId::Llmux,
        path: settings_path.clone(),
        detail: err.to_string(),
    })?;
    Ok(SetupReport {
        provider: crate::provider::ProviderId::Llmux,
        url: Some(url),
        models: Some(catalog.len()),
        model,
        model_reason,
        credential: Some(crate::provider::setup::CredentialSource::KeylessLoopback),
        settings_path,
        overridden_by: overriding_layers(config),
        credential_warning: None,
    })
}

/// What will still outrank the profile once setup has written it.
///
/// Setup writes one layer, and two others sit above it. Silence here would make
/// the receipt a lie in the two cases that are hardest to notice from the
/// outside: a shell variable that keeps winning, and a workspace entry pinning
/// this directory to something else. Neither is an error -- both are things the
/// operator set on purpose -- so this is a warning, not a refusal.
fn overriding_layers(config: &RuntimeConfig) -> Option<String> {
    let mut layers: Vec<String> = Vec::new();
    if config.sources.model == SettingSource::ProcessOverride {
        layers.push(crate::config::ENV_MODEL.to_string());
    }
    if config.sources.model == SettingSource::UserWorkspace
        || config.sources.provider == SettingSource::UserWorkspace
        || config.sources.llmux_url == SettingSource::UserWorkspace
    {
        layers.push(format!(
            "the workspaces entry for {}",
            config.workspace_root.display()
        ));
    }
    (!layers.is_empty()).then(|| layers.join(", "))
}

/// Finds a daemon and returns it with the catalog that proved it.
///
/// The candidates are tried in order and the first that *answers as llmux* wins.
/// An explicit `--url` is not a candidate list: naming a URL and having xfx quietly
/// use a different one would be worse than failing, so a refused explicit URL is
/// the whole answer.
async fn discover(
    config: &RuntimeConfig,
    env: &Environment,
    explicit_url: Option<&str>,
) -> Result<(String, Vec<model::CatalogEntry>), SetupError> {
    if let Some(url) = explicit_url {
        let endpoint = super::endpoint(url, "--url").map_err(SetupError::Endpoint)?;
        let url = trim_base(endpoint.url());
        let catalog = probe(&url).await?;
        return Ok((url, catalog));
    }

    probe_candidates(candidates(config, env)).await
}

/// Probes `candidates` in order and returns the first that answers as llmux.
///
/// Split out and public because the composition is what needed proving and what
/// could not be proven any other way: the first candidate of a real run is
/// `127.0.0.1:3456`, which on a developer's machine is a live daemon, so a test
/// that exercised the fallthrough through the binary would reach the operator's
/// own infrastructure and would pass or fail depending on whether it happened to
/// be running. Over an injected list it is hermetic.
pub async fn discover_in(
    candidates: Vec<String>,
) -> Result<(String, Vec<model::CatalogEntry>), SetupError> {
    probe_candidates(candidates).await
}

/// The candidate loop.
async fn probe_candidates(
    candidates: Vec<String>,
) -> Result<(String, Vec<model::CatalogEntry>), SetupError> {
    let mut tried: Vec<String> = Vec::new();
    let mut answered: Option<String> = None;
    for candidate in candidates {
        match probe(&candidate).await {
            Ok(catalog) => return Ok((candidate, catalog)),
            Err(err) => {
                // A candidate that answered and was not llmux is the one worth
                // repeating: it means something else holds that port.
                if let SetupError::NotLlmux { .. } = &err {
                    answered = Some(err.to_string());
                }
                tried.push(candidate);
            }
        }
    }
    Err(SetupError::NotFound { tried, answered })
}

/// Where xfx will look for a daemon, in order, when nothing named one.
///
/// Three places and no more, most-likely first:
///
/// 1. the url a previous `setup` recorded, which is the one every turn on this
///    machine is already using -- ignoring it meant that re-running setup on a
///    machine whose daemon is not on the default port probed the wrong place
///    first;
/// 2. the default loopback port; and
/// 3. whatever port llmux's own configuration names.
///
/// Trying a range would be a port scan, and trying anything off this machine
/// would be a prompt sent somewhere nobody asked for.
///
/// Pure, and public, and separate from the probing, so the rule can be proven
/// without a socket -- which matters here more than usual, because one of the
/// candidates is the port a real daemon on the developer's own machine is
/// listening on.
pub fn candidates(config: &RuntimeConfig, env: &Environment) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let mut add = |url: String| {
        if !candidates.contains(&url) {
            candidates.push(url);
        }
    };
    // Already validated by the endpoint policy on its way into the config, so
    // this cannot reintroduce a remote or pathed URL.
    if let Some(recorded) = config.llmux_url.as_deref() {
        add(trim_base(recorded));
    }
    add(DEFAULT_URL.to_string());
    if let Some(port) = configured_port(config, env) {
        add(format!("http://127.0.0.1:{port}"));
    }
    candidates
}

/// Both halves of the identification, in order.
async fn probe(base: &str) -> Result<Vec<model::CatalogEntry>, SetupError> {
    let refuse = |detail: String| SetupError::NotLlmux {
        url: base.to_string(),
        detail,
    };

    // Check that GET / returns "llmux" to identify the daemon.
    let root = fetch_root(base).await.map_err(&refuse)?;
    if root.trim() != ROOT_BODY {
        return Err(refuse(format!(
            "`GET /` answered {:?} rather than `{ROOT_BODY}`, so something else is on this port",
            clip(&root)
        )));
    }

    // Fetch the catalog using the shared fetch_catalog function.
    let catalog_url = format!("{base}/models");
    model::fetch_catalog(&catalog_url).await.map_err(|err| {
        refuse(match err {
            model::CatalogError::Empty => {
                "`GET /models` answered an empty catalog, so the daemon has no model to use"
                    .to_string()
            }
            model::CatalogError::Malformed { detail } => {
                format!("`GET /models` did not answer a model catalog: {detail}")
            }
            model::CatalogError::Unavailable { detail } => detail,
        })
    })
}

/// Fetches the root endpoint to identify the daemon using the shared loopback helper.
async fn fetch_root(base: &str) -> Result<String, String> {
    model::fetch_loopback_bounded(&format!("{base}/"), 1024 * 1024).await
}

/// Keeps the configured model when the daemon has it, else takes the first.
///
/// Keeping it matters because the model is the one setting an operator is most
/// likely to have chosen deliberately, and a setup command that silently
/// replaced it would be a setup command that undid a decision. Replacing it when
/// the daemon does not have it matters for the same reason in reverse: leaving a
/// model the daemon will reject would record a configuration that cannot run.
fn select_model(configured: &str, catalog: &[model::CatalogEntry]) -> (String, String) {
    if catalog.iter().any(|entry| entry.matches(configured)) {
        return (
            configured.to_string(),
            format!("kept `{configured}`, which this daemon offers"),
        );
    }
    let first = catalog
        .first()
        .expect("an empty catalog was refused by the probe");
    let chosen = first.preferred_name().to_string();
    (
        chosen.clone(),
        format!("`{configured}` is not in this daemon's catalog, so xfx chose its first entry"),
    )
}

/// The `proxy.port` from llmux's own configuration file, if there is one.
///
/// **Exactly one field is read.** That file holds OAuth tokens and admin keys
/// beside the port, and nothing in xfx has a reason to see them: the port is
/// parsed out and the document is dropped, so no other field can reach a log, a
/// snapshot, or the settings file this command writes.
fn configured_port(config: &RuntimeConfig, env: &Environment) -> Option<u16> {
    let path = llmux_config_path(config, env)?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SETTINGS_BYTES as u64 {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    let document: Value = serde_json::from_str(&text).ok()?;
    let port = document.get("proxy")?.get("port")?.as_u64()?;
    u16::try_from(port).ok().filter(|port| *port != 0)
}

/// Where llmux keeps its configuration, by its own documented precedence.
fn llmux_config_path(config: &RuntimeConfig, env: &Environment) -> Option<PathBuf> {
    if let Some(explicit) = env.var(ENV_LLMUX_CONFIG).map(str::trim) {
        if !explicit.is_empty() {
            return Some(PathBuf::from(explicit));
        }
    }
    if let Some(xdg) = env.var(ENV_XDG_CONFIG_HOME).map(str::trim) {
        if !xdg.is_empty() {
            return Some(Path::new(xdg).join(LLMUX_CONFIG_FILE));
        }
    }
    // `~/.config/llmux.json`. The profile home is `~/.xfx`, so its parent is the
    // home directory xfx was told to use -- the same one every other path here
    // is derived from, rather than a second reading of the environment.
    let home = config.profile_dir.as_deref()?.parent()?;
    Some(home.join(".config").join(LLMUX_CONFIG_FILE))
}

/// A base URL with any trailing `/` removed.
fn trim_base(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// How much of an unexpected body is quoted back.
fn clip(body: &str) -> String {
    const LIMIT: usize = 120;
    let trimmed = body.trim();
    if trimmed.len() <= LIMIT {
        return trimmed.to_string();
    }
    let mut end = LIMIT;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn entries(pairs: &[(&str, &[&str])]) -> Vec<model::CatalogEntry> {
        pairs
            .iter()
            .map(|(id, aliases)| model::CatalogEntry {
                id: (*id).to_string(),
                aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
                name: None,
                efforts: Vec::new(),
                max_context: None,
            })
            .collect()
    }

    #[test]
    fn a_catalog_parses_into_ids_and_aliases() {
        let body = json!({
            "models": [
                { "id": "claude-fable-5[1m]", "aliases": ["fable"], "group": "anthropic" },
                { "id": "bare", "aliases": [] },
                { "id": "", "aliases": ["skipped"] },
                { "aliases": ["no-id"] },
                "not an object",
            ],
        })
        .to_string();
        assert_eq!(
            model::parse_llmux_catalog(&body),
            Some(entries(&[
                ("claude-fable-5[1m]", &["fable"]),
                ("bare", &[])
            ])),
            "an entry xfx cannot name is skipped rather than invented"
        );
    }

    #[test]
    fn a_body_that_is_not_a_catalog_is_not_one() {
        assert_eq!(model::parse_llmux_catalog("not json"), None);
        assert_eq!(model::parse_llmux_catalog("[]"), None);
        assert_eq!(
            model::parse_llmux_catalog(&json!({ "models": 7 }).to_string()),
            None
        );
        assert_eq!(
            model::parse_llmux_catalog(&json!({ "models": [] }).to_string()),
            Some(Vec::new()),
            "an empty catalog is a readable answer; the probe is what refuses it"
        );
    }

    #[test]
    fn a_configured_model_the_daemon_has_is_kept_by_id_or_alias() {
        let catalog = entries(&[("a-id", &["a", "second"]), ("b-id", &[])]);
        for name in ["a-id", "a", "second", "b-id"] {
            let (model, reason) = select_model(name, &catalog);
            assert_eq!(model, name);
            assert!(reason.contains("kept"), "{reason}");
        }
    }

    #[test]
    fn a_model_the_daemon_does_not_have_is_replaced_by_its_first_entry() {
        let catalog = entries(&[("a-id", &["a"]), ("b-id", &[])]);
        let (model, reason) = select_model("vendor/absent", &catalog);
        assert_eq!(model, "a", "the first entry, by its published short name");
        assert!(reason.contains("vendor/absent"), "{reason}");

        let (model, _) = select_model("vendor/absent", &entries(&[("only-an-id", &[])]));
        assert_eq!(model, "only-an-id", "an entry with no alias uses its id");
    }

    #[test]
    fn only_the_port_is_read_out_of_the_llmux_configuration() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(LLMUX_CONFIG_FILE);
        fs::write(
            &path,
            json!({
                "proxy": { "port": 3456, "host": "0.0.0.0" },
                "accounts": [{ "oauth_token": "secret" }],
            })
            .to_string(),
        )
        .expect("write config");

        let config = test_config(dir.path());
        let env = Environment::new(
            None,
            std::collections::BTreeMap::from([(
                ENV_LLMUX_CONFIG.to_string(),
                path.display().to_string(),
            )]),
        );
        assert_eq!(configured_port(&config, &env), Some(3456));
    }

    #[test]
    fn a_configuration_without_a_usable_port_names_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = test_config(dir.path());
        for body in [
            json!({}).to_string(),
            json!({ "proxy": {} }).to_string(),
            json!({ "proxy": { "port": 0 } }).to_string(),
            json!({ "proxy": { "port": 70000 } }).to_string(),
            json!({ "proxy": { "port": "3456" } }).to_string(),
            "not json".to_string(),
        ] {
            let path = dir.path().join(LLMUX_CONFIG_FILE);
            fs::write(&path, &body).expect("write config");
            let env = Environment::new(
                None,
                std::collections::BTreeMap::from([(
                    ENV_LLMUX_CONFIG.to_string(),
                    path.display().to_string(),
                )]),
            );
            assert_eq!(configured_port(&config, &env), None, "for {body}");
        }
    }

    #[test]
    fn the_config_path_follows_llmuxs_own_precedence() {
        let config = test_config(Path::new("/home/someone"));
        let with = |vars: &[(&str, &str)]| {
            let map = vars
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            llmux_config_path(&config, &Environment::new(None, map))
        };
        assert_eq!(
            with(&[(ENV_LLMUX_CONFIG, "/explicit/somewhere.json")]),
            Some(PathBuf::from("/explicit/somewhere.json"))
        );
        assert_eq!(
            with(&[(ENV_XDG_CONFIG_HOME, "/xdg")]),
            Some(PathBuf::from("/xdg/llmux.json"))
        );
        assert_eq!(
            with(&[]),
            Some(PathBuf::from("/home/someone/.config/llmux.json"))
        );
        // A blank value is ignored rather than treated as a path, matching every
        // other environment knob xfx reads.
        assert_eq!(
            with(&[(ENV_LLMUX_CONFIG, "  "), (ENV_XDG_CONFIG_HOME, "/xdg")]),
            Some(PathBuf::from("/xdg/llmux.json"))
        );
    }

    #[test]
    fn discovery_tries_the_default_port_then_the_one_llmux_configured() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = test_config(dir.path());
        let path = dir.path().join(LLMUX_CONFIG_FILE);
        let env_naming = |path: &Path| {
            Environment::new(
                None,
                std::collections::BTreeMap::from([(
                    ENV_LLMUX_CONFIG.to_string(),
                    path.display().to_string(),
                )]),
            )
        };

        // Nothing configured: the default is the whole list. In particular there
        // is no scan -- two candidates at most, ever.
        assert_eq!(
            candidates(&config, &env_naming(&path)),
            vec![DEFAULT_URL.to_string()]
        );

        fs::write(&path, json!({ "proxy": { "port": 4123 } }).to_string()).expect("write");
        assert_eq!(
            candidates(&config, &env_naming(&path)),
            vec![DEFAULT_URL.to_string(), "http://127.0.0.1:4123".to_string()],
            "the default is tried first, then the configured port"
        );

        // A configuration that names the default port adds nothing: probing the
        // same URL twice would report it as two failures.
        fs::write(&path, json!({ "proxy": { "port": 3456 } }).to_string()).expect("write");
        assert_eq!(
            candidates(&config, &env_naming(&path)),
            vec![DEFAULT_URL.to_string()]
        );
    }

    #[test]
    fn a_failed_discovery_names_every_place_it_looked() {
        let message = SetupError::NotFound {
            tried: vec![DEFAULT_URL.to_string(), "http://127.0.0.1:4123".to_string()],
            answered: None,
        }
        .to_string();
        assert!(message.contains(DEFAULT_URL), "{message}");
        assert!(message.contains("http://127.0.0.1:4123"), "{message}");
        assert!(message.contains("--url"), "{message}");
    }

    #[test]
    fn a_failed_discovery_repeats_what_actually_answered() {
        // Otherwise a port conflict reads as "nothing is running" and sends the
        // operator to start a daemon that is already up.
        let message = SetupError::NotFound {
            tried: vec![DEFAULT_URL.to_string()],
            answered: Some(
                "`http://127.0.0.1:3456` did not answer as an llmux daemon: nginx".to_string(),
            ),
        }
        .to_string();
        assert!(message.contains("nginx"), "{message}");
        assert!(message.contains("--url"), "{message}");
    }

    #[test]
    fn a_client_that_could_not_be_built_does_not_blame_a_daemon() {
        let message = SetupError::Client {
            detail: "no tls backend".to_string(),
        }
        .to_string();
        assert!(message.contains("xfx"), "{message}");
        assert!(!message.contains("did not answer"), "{message}");
    }

    #[test]
    fn an_existing_settings_file_that_cannot_be_read_is_never_replaced() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        for body in ["{ broken", "[]", "\"a string\""] {
            fs::write(&path, body).expect("write settings");
            assert!(
                profile::read_existing(&path).is_err(),
                "`{body}` must be a refusal"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), body, "bytes untouched");
        }
        fs::remove_file(&path).expect("remove");
        assert_eq!(
            profile::read_existing(&path).expect("absent is empty"),
            Map::new()
        );
    }

    /// Every `*.staged` name directly under `dir`, sorted.
    fn staged_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read the directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(crate::session::STAGE_SUFFIX))
            .collect();
        names.sort();
        names
    }

    /// The stage name shape this writer used before it carried a nonce.
    fn pid_only_stage(dir: &Path) -> PathBuf {
        dir.join(format!("settings.json.{}.staged", std::process::id()))
    }

    #[test]
    fn a_stage_another_writer_owns_is_never_written_through() {
        // The stage name used to be this process's pid and nothing else, opened
        // with `create().truncate()`. Two writes -- a second xfx, or this one
        // twice -- would then share a name, and the second would write straight
        // through a file the first was still filling. A nonce is what makes that
        // impossible, so this plants a file at the *old* name shape and requires
        // it to survive a successful write untouched.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let foreign = pid_only_stage(dir.path());
        fs::write(&foreign, b"another writer owns this").expect("plant a stage");

        profile::replace_private_file(dir.path(), &path, b"{\"ok\":true}\n")
            .expect("the write succeeds");

        assert_eq!(
            fs::read_to_string(&foreign).unwrap(),
            "another writer owns this",
            "a stage this write did not create must not be written through"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"ok\":true}\n");
    }

    #[test]
    fn a_successful_write_leaves_no_stage_of_its_own_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        profile::replace_private_file(dir.path(), &path, b"{}\n").expect("the write succeeds");
        assert!(
            staged_files(dir.path()).is_empty(),
            "{:?}",
            staged_files(dir.path())
        );
    }

    #[test]
    fn a_failed_rename_removes_this_writes_stage_and_no_other() {
        // Renaming onto a directory fails, which is the reachable way to reach
        // the guard. The stage this write created must be gone; a stage it did
        // not create must still be there, because unlinking a name you do not
        // own is the interference the nonce exists to prevent.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::create_dir(&path).expect("a target that cannot be renamed onto");
        let foreign = pid_only_stage(dir.path());
        fs::write(&foreign, b"another writer owns this").expect("plant a stage");

        profile::replace_private_file(dir.path(), &path, b"{}\n")
            .expect_err("renaming onto a directory fails");

        assert_eq!(
            fs::read_to_string(&foreign).unwrap(),
            "another writer owns this",
            "the guard must never unlink a stage this write did not create"
        );
        assert_eq!(
            staged_files(dir.path()),
            vec![foreign.file_name().unwrap().to_string_lossy().into_owned()],
            "this write's own stage is gone and only the foreign one remains"
        );
    }

    #[test]
    fn a_long_unexpected_body_is_clipped_on_a_character_boundary() {
        let clipped = clip(&"한".repeat(200));
        assert!(clipped.ends_with("..."), "{clipped}");
        assert!(clipped.len() <= 123, "{}", clipped.len());
    }

    #[test]
    fn llmux_error_messages_name_the_provider() {
        // Verify that error messages are provider-aware and mention "llmux"
        let settings_path = std::path::PathBuf::from("/tmp/settings.json");

        // UnreadableSettings
        let unreadable = SetupError::UnreadableSettings {
            provider: crate::provider::ProviderId::Llmux,
            path: settings_path.clone(),
            detail: "permission denied".to_string(),
        }
        .to_string();
        assert!(
            unreadable.contains("xfx setup llmux"),
            "UnreadableSettings should mention llmux: {unreadable}"
        );
        assert!(
            !unreadable.contains("gateway"),
            "UnreadableSettings should not mention gateway: {unreadable}"
        );

        // Write
        let write_err = SetupError::Write {
            provider: crate::provider::ProviderId::Llmux,
            path: settings_path.clone(),
            detail: "no space".to_string(),
        }
        .to_string();
        assert!(
            write_err.contains("cannot write"),
            "Write should contain error: {write_err}"
        );

        // NoProfile
        let no_profile = SetupError::NoProfile {
            provider: crate::provider::ProviderId::Llmux,
        }
        .to_string();
        assert!(
            no_profile.contains("daemon"),
            "NoProfile should mention daemon for llmux: {no_profile}"
        );
        assert!(
            !no_profile.contains("gateway"),
            "NoProfile should not mention gateway: {no_profile}"
        );
    }

    #[test]
    fn llmux_unreadable_settings_exact_message() {
        // Exact-string regression test: llmux UnreadableSettings message byte-identical
        let settings_path = std::path::PathBuf::from("/tmp/settings.json");
        let unreadable = SetupError::UnreadableSettings {
            provider: crate::provider::ProviderId::Llmux,
            path: settings_path,
            detail: "permission denied".to_string(),
        }
        .to_string();
        // This exact message must not change without a migration notice
        assert_eq!(
            unreadable,
            "xfx will not replace /tmp/settings.json: permission denied. Fix or move the file and run `xfx setup llmux` again -- xfx does not overwrite settings it could not read"
        );
    }

    #[test]
    fn llmux_noprofile_exact_message() {
        // Exact-string regression test: llmux NoProfile message must preserve legacy "record the daemon" text
        let no_profile = SetupError::NoProfile {
            provider: crate::provider::ProviderId::Llmux,
        }
        .to_string();
        // Legacy message must be preserved byte-identical for llmux
        assert_eq!(
            no_profile,
            "xfx cannot record the daemon because no home directory is set, so there is no `~/.xfx/settings.json` to write"
        );
    }

    #[test]
    fn gateway_error_messages_name_the_provider_all_variants() {
        // Verify that gateway errors mention "gateway", not "llmux" or "daemon"
        let settings_path = std::path::PathBuf::from("/tmp/settings.json");

        // Gateway UnreadableSettings
        let unreadable = SetupError::UnreadableSettings {
            provider: crate::provider::ProviderId::Gateway,
            path: settings_path.clone(),
            detail: "permission denied".to_string(),
        }
        .to_string();
        assert!(
            unreadable.contains("xfx setup gateway"),
            "UnreadableSettings should mention gateway: {unreadable}"
        );
        assert!(
            !unreadable.contains("daemon"),
            "UnreadableSettings should not mention daemon: {unreadable}"
        );

        // Gateway Write error
        let write_err = SetupError::Write {
            provider: crate::provider::ProviderId::Gateway,
            path: settings_path.clone(),
            detail: "no space".to_string(),
        }
        .to_string();
        assert!(
            write_err.contains("cannot write"),
            "Write should contain error: {write_err}"
        );

        // Gateway NoProfile
        let no_profile = SetupError::NoProfile {
            provider: crate::provider::ProviderId::Gateway,
        }
        .to_string();
        assert!(
            no_profile.contains("gateway"),
            "NoProfile should mention gateway: {no_profile}"
        );
        assert!(
            !no_profile.contains("daemon"),
            "NoProfile should not mention daemon: {no_profile}"
        );
        assert!(
            !no_profile.contains("llmux"),
            "NoProfile should not mention llmux: {no_profile}"
        );
    }

    /// A config rooted at `home`, built the way the loader builds one.
    fn test_config(home: &Path) -> RuntimeConfig {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let mut config = RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), std::collections::BTreeMap::new()),
            workspace.path(),
        )
        .expect("load config");
        // `load_with` derives the profile dir from the home it was given; keep
        // it, and let the workspace temp dir drop.
        config.workspace_root = PathBuf::from("/");
        config
    }
}
