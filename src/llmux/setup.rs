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
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

use crate::config::{
    Environment, RuntimeConfig, ENV_LLMUX_CONFIG, ENV_XDG_CONFIG_HOME, MAX_SETTINGS_BYTES,
};
use crate::gateway::{Endpoint, EndpointError, USER_AGENT};

use super::DEFAULT_URL;

/// How long the probe may take to open a connection.
///
/// Short, because the thing being probed is on this machine: a loopback connect
/// that has not completed in three seconds is not slow, it is absent, and the
/// discovery path has another candidate to try.
const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the probe waits for a local daemon to answer.
const PROBE_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest probe response xfx will read.
///
/// The catalog is a list of model descriptors; a body past this is not a catalog
/// and reading it would let whatever is on that port decide how much memory this
/// command uses.
const MAX_PROBE_BODY_BYTES: usize = 1024 * 1024;

/// The body `GET /` answers on a real daemon
/// (`2lab-ai/llmux src/proxy/server.rs:1240`).
const ROOT_BODY: &str = "llmux";

/// llmux's configuration file name, under whichever directory holds it.
const LLMUX_CONFIG_FILE: &str = "llmux.json";

/// What `setup` decided, once it has decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupReport {
    /// The daemon xfx will talk to.
    pub url: String,
    /// How many models the daemon offers. A count rather than the catalog: the
    /// human line says "catalog size" and the two surfaces must agree, and a
    /// document that grew with the daemon's model list would be unbounded output.
    pub models: usize,
    /// The model that was recorded.
    pub model: String,
    /// Why that model, in one line.
    pub model_reason: String,
    /// The settings file that was written.
    pub settings_path: PathBuf,
}

/// Why `setup` could not finish.
#[derive(Debug)]
pub enum SetupError {
    /// The `--url` the invocation named is not one xfx may send a prompt to.
    Endpoint(EndpointError),
    /// No daemon answered, and nothing named another place to look.
    NotFound { tried: Vec<String> },
    /// Something answered, but it is not llmux.
    NotLlmux { url: String, detail: String },
    /// There is no home directory, so there is no profile to write.
    NoProfile,
    /// The existing settings file could not be read, so it must not be replaced.
    UnreadableSettings { path: PathBuf, detail: String },
    /// The settings file could not be written.
    Write { path: PathBuf, detail: String },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Endpoint(err) => write!(f, "{err}"),
            Self::NotFound { tried } => write!(
                f,
                "no llmux daemon answered. xfx tried {}. Start llmux, or name it with \
                 `xfx setup llmux --url http://127.0.0.1:<port>`",
                tried.join(", ")
            ),
            Self::NotLlmux { url, detail } => {
                write!(f, "`{url}` did not answer as an llmux daemon: {detail}")
            }
            Self::NoProfile => write!(
                f,
                "xfx cannot record the daemon because no home directory is set, \
                 so there is no `~/.xfx/settings.json` to write"
            ),
            Self::UnreadableSettings { path, detail } => write!(
                f,
                "xfx will not replace {}: {detail}. Fix or move the file and run \
                 `xfx setup llmux` again -- xfx does not overwrite settings it could not read",
                path.display()
            ),
            Self::Write { path, detail } => {
                write!(f, "cannot write {}: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for SetupError {}

/// Runs the whole command: discover, probe, choose, record.
pub async fn run(
    config: &RuntimeConfig,
    env: &Environment,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError> {
    let (url, catalog) = discover(config, env, explicit_url).await?;
    let (model, model_reason) = select_model(&config.model, &catalog);
    let settings_path = config
        .user_settings_path
        .clone()
        .ok_or(SetupError::NoProfile)?;
    record(&settings_path, &url, &model)?;
    Ok(SetupReport {
        url,
        models: catalog.len(),
        model,
        model_reason,
        settings_path,
    })
}

/// One catalog entry, reduced to the two names a model may be called by.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogEntry {
    id: String,
    aliases: Vec<String>,
}

impl CatalogEntry {
    /// Whether `name` selects this entry, as an id or as an alias.
    fn matches(&self, name: &str) -> bool {
        self.id == name || self.aliases.iter().any(|alias| alias == name)
    }

    /// How xfx will ask for this model.
    ///
    /// The first alias when there is one, because that is the short name the
    /// daemon publishes for it and the one an operator recognizes; the id
    /// otherwise. Either resolves at the daemon.
    fn preferred_name(&self) -> &str {
        self.aliases.first().map(String::as_str).unwrap_or(&self.id)
    }
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
) -> Result<(String, Vec<CatalogEntry>), SetupError> {
    let client = probe_client()?;

    if let Some(url) = explicit_url {
        let endpoint = Endpoint::checked(url, "--url").map_err(SetupError::Endpoint)?;
        let url = trim_base(endpoint.url());
        let catalog = probe(&client, &url).await?;
        return Ok((url, catalog));
    }

    let mut tried: Vec<String> = Vec::new();
    for candidate in candidates(config, env) {
        match probe(&client, &candidate).await {
            Ok(catalog) => return Ok((candidate, catalog)),
            Err(_) => tried.push(candidate),
        }
    }
    Err(SetupError::NotFound { tried })
}

/// Where xfx will look for a daemon, in order, when nothing named one.
///
/// Two places and no more: the default loopback port, then whatever port llmux's
/// own configuration names. Trying a range would be a port scan, and trying
/// anything off this machine would be a prompt sent somewhere nobody asked for.
///
/// Pure, and separate from the probing, so the rule can be proven without a
/// socket -- which matters here more than usual, because the first candidate is
/// the port a real daemon on the developer's own machine is listening on.
fn candidates(config: &RuntimeConfig, env: &Environment) -> Vec<String> {
    let mut candidates = vec![DEFAULT_URL.to_string()];
    if let Some(port) = configured_port(config, env) {
        let from_config = format!("http://127.0.0.1:{port}");
        if !candidates.contains(&from_config) {
            candidates.push(from_config);
        }
    }
    candidates
}

fn probe_client() -> Result<reqwest::Client, SetupError> {
    reqwest::Client::builder()
        .connect_timeout(PROBE_CONNECT_TIMEOUT)
        .read_timeout(PROBE_READ_TIMEOUT)
        .timeout(PROBE_READ_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|err| SetupError::NotLlmux {
            url: String::new(),
            detail: err.to_string(),
        })
}

/// Both halves of the identification, in order.
async fn probe(client: &reqwest::Client, base: &str) -> Result<Vec<CatalogEntry>, SetupError> {
    let refuse = |detail: String| SetupError::NotLlmux {
        url: base.to_string(),
        detail,
    };

    let root = fetch(client, &format!("{base}/")).await.map_err(&refuse)?;
    if root.trim() != ROOT_BODY {
        return Err(refuse(format!(
            "`GET /` answered {:?} rather than `{ROOT_BODY}`, so something else is on this port",
            clip(&root)
        )));
    }

    let body = fetch(client, &format!("{base}/models"))
        .await
        .map_err(&refuse)?;
    let catalog = parse_catalog(&body).ok_or_else(|| {
        refuse(format!(
            "`GET /models` did not answer a model catalog: {:?}",
            clip(&body)
        ))
    })?;
    if catalog.is_empty() {
        return Err(refuse(
            "`GET /models` answered an empty catalog, so the daemon has no model to use"
                .to_string(),
        ));
    }
    Ok(catalog)
}

/// One bounded GET that must answer 2xx.
async fn fetch(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    let take = bytes.len().min(MAX_PROBE_BODY_BYTES);
    Ok(String::from_utf8_lossy(&bytes[..take]).into_owned())
}

/// The `{"models":[{id,name,aliases,...}]}` document, reduced to names.
///
/// `None` when the body is not that document at all. An entry without a usable
/// id is skipped rather than invented: a model xfx cannot name is a model it
/// cannot ask for.
fn parse_catalog(body: &str) -> Option<Vec<CatalogEntry>> {
    let Ok(Value::Object(document)) = serde_json::from_str::<Value>(body) else {
        return None;
    };
    let Some(Value::Array(models)) = document.get("models") else {
        return None;
    };
    Some(
        models
            .iter()
            .filter_map(|model| {
                let object = model.as_object()?;
                let id = object.get("id")?.as_str()?;
                if id.is_empty() {
                    return None;
                }
                let aliases = match object.get("aliases") {
                    Some(Value::Array(aliases)) => aliases
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|alias| !alias.is_empty())
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                Some(CatalogEntry {
                    id: id.to_string(),
                    aliases,
                })
            })
            .collect(),
    )
}

/// Keeps the configured model when the daemon has it, else takes the first.
///
/// Keeping it matters because the model is the one setting an operator is most
/// likely to have chosen deliberately, and a setup command that silently
/// replaced it would be a setup command that undid a decision. Replacing it when
/// the daemon does not have it matters for the same reason in reverse: leaving a
/// model the daemon will reject would record a configuration that cannot run.
fn select_model(configured: &str, catalog: &[CatalogEntry]) -> (String, String) {
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

/// Merges the three keys into the profile settings and writes them privately.
fn record(path: &Path, url: &str, model: &str) -> Result<(), SetupError> {
    let mut settings = read_existing(path)?;
    settings.insert("backend".to_string(), Value::from("llmux"));
    settings.insert("llmux_url".to_string(), Value::from(url));
    settings.insert("model".to_string(), Value::from(model));

    let mut body = serde_json::to_string_pretty(&Value::Object(settings))
        .expect("a settings object is always serializable");
    body.push('\n');

    let dir = path.parent().ok_or_else(|| SetupError::Write {
        path: path.to_path_buf(),
        detail: "the settings path has no parent directory".to_string(),
    })?;
    create_private_dir(dir).map_err(|err| SetupError::Write {
        path: dir.to_path_buf(),
        detail: err.to_string(),
    })?;
    replace_private_file(dir, path, body.as_bytes()).map_err(|err| SetupError::Write {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })
}

/// The settings object already on disk, or an empty one when there is none.
///
/// A file that exists and cannot be read is a refusal, never an empty object.
/// "xfx could not parse this" and "this is not worth keeping" are different
/// claims, and only the operator gets to make the second one.
fn read_existing(path: &Path) -> Result<Map<String, Value>, SetupError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(err) => {
            return Err(SetupError::UnreadableSettings {
                path: path.to_path_buf(),
                detail: err.to_string(),
            })
        }
    };
    if !metadata.is_file() {
        return Err(SetupError::UnreadableSettings {
            path: path.to_path_buf(),
            detail: "it is not a regular file".to_string(),
        });
    }
    if metadata.len() > MAX_SETTINGS_BYTES as u64 {
        return Err(SetupError::UnreadableSettings {
            path: path.to_path_buf(),
            detail: format!("it is larger than {MAX_SETTINGS_BYTES} bytes"),
        });
    }
    let text = fs::read_to_string(path).map_err(|err| SetupError::UnreadableSettings {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(SetupError::UnreadableSettings {
            path: path.to_path_buf(),
            detail: "it is not a JSON object".to_string(),
        }),
        Err(err) => Err(SetupError::UnreadableSettings {
            path: path.to_path_buf(),
            detail: format!("it is not valid JSON: {err}"),
        }),
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

/// Writes `bytes` to `path` atomically and privately.
///
/// The discipline is the session store's (`src/session/store.rs`
/// `replace_private_file`), for the same reasons: the stage lives in the target
/// directory so the rename cannot cross a filesystem, the stage name carries
/// this process's id so a concurrent writer's file is never removed, the file is
/// created `0600` rather than tightened afterwards, and the directory is synced
/// so the *name* is durable and not only the bytes it points at.
///
/// A reader of `settings.json` therefore sees either the old document or the new
/// one -- never a half-written file, which for a settings file would mean the
/// next `xfx` run silently loses every key past the truncation.
fn replace_private_file(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = dir.join(format!("settings.json.{}.staged", std::process::id()));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged)?;
        io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    // A rename onto the target keeps the target's inode replaced atomically;
    // a failure leaves the stage behind rather than a truncated settings file.
    match fs::rename(&staged, path) {
        Ok(()) => {}
        Err(err) => {
            let _ = fs::remove_file(&staged);
            return Err(err);
        }
    }
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
    use serde_json::json;

    fn entries(pairs: &[(&str, &[&str])]) -> Vec<CatalogEntry> {
        pairs
            .iter()
            .map(|(id, aliases)| CatalogEntry {
                id: (*id).to_string(),
                aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
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
            parse_catalog(&body),
            Some(entries(&[
                ("claude-fable-5[1m]", &["fable"]),
                ("bare", &[])
            ])),
            "an entry xfx cannot name is skipped rather than invented"
        );
    }

    #[test]
    fn a_body_that_is_not_a_catalog_is_not_one() {
        assert_eq!(parse_catalog("not json"), None);
        assert_eq!(parse_catalog("[]"), None);
        assert_eq!(parse_catalog(&json!({ "models": 7 }).to_string()), None);
        assert_eq!(
            parse_catalog(&json!({ "models": [] }).to_string()),
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
        }
        .to_string();
        assert!(message.contains(DEFAULT_URL), "{message}");
        assert!(message.contains("http://127.0.0.1:4123"), "{message}");
        assert!(message.contains("--url"), "{message}");
    }

    #[test]
    fn an_existing_settings_file_that_cannot_be_read_is_never_replaced() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        for body in ["{ broken", "[]", "\"a string\""] {
            fs::write(&path, body).expect("write settings");
            assert!(
                matches!(
                    read_existing(&path),
                    Err(SetupError::UnreadableSettings { .. })
                ),
                "`{body}` must be a refusal"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), body, "bytes untouched");
        }
        fs::remove_file(&path).expect("remove");
        assert_eq!(read_existing(&path).expect("absent is empty"), Map::new());
    }

    #[test]
    fn a_long_unexpected_body_is_clipped_on_a_character_boundary() {
        let clipped = clip(&"한".repeat(200));
        assert!(clipped.ends_with("..."), "{clipped}");
        assert!(clipped.len() <= 123, "{}", clipped.len());
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
