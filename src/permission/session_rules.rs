//! Saved-session permission rules: exact actions a person told one saved
//! session to allow or deny, with ids and generations.
//!
//! Upstream's store (`vercel-labs/fx@c1db9191
//! src/core/permissions/session_permission_state.zig`), kept beside the
//! session's log as `permissions.json` in upstream's shape
//! (`session_codec.zig:995-1091`):
//!
//! ```json
//! {"schema_version":2,"next_generation":3,"rules":[
//!   {"id":1,"kind":"file_mutation","canonical":{"encoding":"base64","data":"…"},
//!    "display_identity":"write_file b.txt preimage=absent after=a1fce4363854ff88",
//!    "decision":"deny","generation":1}]}
//! ```
//!
//! A rule is about **one exact prepared action**, not a pattern: its key is
//! the bytes that say what would run -- a command's text, directory, mode and
//! platform; a change's tool, typed input, canonical target, preimage, the
//! digest of what it would leave and the identity of every directory it
//! touches -- so it can only ever match the action it was made from. It is
//! `/permissions remember`'s and `/permissions revoke`'s to change, each
//! confirmed first, and each a compare-and-swap on the rule's generation: a
//! confirmation that saw an older rule than the file now holds is refused
//! (`stale`) rather than applied over a change it never saw.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The schema this build reads and writes (`schema_version`, `:6`).
pub const SCHEMA_VERSION: u64 = 2;
/// The most rules one session keeps (`max_rules`, `:7`).
pub const MAX_RULES: usize = 1024;
/// The largest key (`max_identity_bytes`, `:8`).
pub const MAX_IDENTITY_BYTES: usize = 4096;
/// The largest display (`max_display_identity_bytes`, `:9`).
pub const MAX_DISPLAY_BYTES: usize = 4096;
/// The largest file (`max_permission_state_bytes`, `session_codec.zig:333`).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// The file's name in a session's directory (`session_log.zig:37`).
pub const FILE_NAME: &str = "permissions.json";
/// The lock a change holds while it reads, applies and writes the file.
const LOCK_NAME: &str = "permissions.lock";

/// What a key is about (`RuleKey.Kind`, `:15-20`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Command,
    FileMutation,
    StructuredTool,
}

impl KeyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::FileMutation => "file_mutation",
            Self::StructuredTool => "structured_tool",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "command" => Some(Self::Command),
            "file_mutation" => Some(Self::FileMutation),
            "structured_tool" => Some(Self::StructuredTool),
            _ => None,
        }
    }
}

/// The exact identity of one action (`RuleKey`, `:22-44`): its kind and its
/// canonical bytes. Two keys are the same action only when both are equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleKey {
    pub kind: KeyKind,
    pub canonical: Vec<u8>,
}

impl RuleKey {
    /// A key, or `None` when the bytes are empty or longer than a key may be.
    pub fn new(kind: KeyKind, canonical: Vec<u8>) -> Option<Self> {
        (!canonical.is_empty() && canonical.len() <= MAX_IDENTITY_BYTES)
            .then_some(Self { kind, canonical })
    }

    /// A command's key (`commandKeyV2`, `:197-215`): the command, the
    /// directory it runs in, how it runs, and the platform.
    pub fn command(command: &str, cwd: &Path, target_os: &str) -> Option<Self> {
        let mut canonical = Vec::new();
        field(&mut canonical, b"fx-permission-state-v2");
        field(&mut canonical, command.as_bytes());
        field(&mut canonical, &os_bytes(cwd.as_os_str()));
        field(&mut canonical, b"foreground");
        field(&mut canonical, target_os.as_bytes());
        Self::new(KeyKind::Command, canonical)
    }
}

/// One length-prefixed field: eight big-endian bytes of length, then the
/// bytes (`writeIdentityField`, `:180-187`), so no two field lists can run
/// together into the same bytes.
pub fn field(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

#[cfg(unix)]
pub fn os_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(not(unix))]
pub fn os_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().into_owned().into_bytes()
}

/// The platform word a command key carries (`@tagName(target_os)`).
pub fn target_os() -> &'static str {
    std::env::consts::OS
}

/// What a rule says (`Decision`, `:46-49`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// One saved rule (`Rule`, `:57-68`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: u64,
    pub key: RuleKey,
    pub display: String,
    pub decision: Decision,
    pub generation: u64,
}

/// A session's whole store (`State`, `:70-82`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub next_generation: u64,
    pub rules: Vec<Rule>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            next_generation: 1,
            rules: Vec::new(),
        }
    }
}

/// A change a person confirmed (`ConfirmedRuleEvent`, `:308-322`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// `remember`: this key now decides this way. `expected` is the
    /// generation of the rule the confirmation was shown, or `None` when it
    /// was shown none.
    Set {
        key: RuleKey,
        display: String,
        decision: Decision,
        expected: Option<u64>,
    },
    /// `revoke`: the rule with this id, at this generation, goes.
    Revoke { id: u64, expected: u64 },
}

/// What applying an event came to (`ApplyResult`, `:355-374`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Applied(State),
    /// The store moved since the confirmation was shown.
    Stale,
    /// There is no room for another rule.
    Full,
    /// The store or the event breaks an invariant.
    Invalid,
}

/// What a store says about one action (`StateDecision`, `:51-55`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    Unresolved,
}

impl State {
    /// `validateSchema` (`:88-121`): ids and generations nonzero and below the
    /// next generation, an id no later than its generation, keys and displays
    /// inside their bounds, and no id or key twice.
    pub fn validate(&self) -> Result<(), String> {
        let invalid = |why: &str| {
            Err(format!(
                "the saved-session permission rules are invalid: {why}"
            ))
        };
        if self.next_generation == 0 {
            return invalid("next_generation is 0");
        }
        if self.rules.len() > MAX_RULES {
            return invalid("there are more rules than a session may keep");
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.id == 0
                || rule.generation == 0
                || rule.id > rule.generation
                || rule.id >= self.next_generation
                || rule.generation >= self.next_generation
            {
                return invalid("a rule's id or generation is out of order");
            }
            if rule.key.canonical.is_empty() || rule.key.canonical.len() > MAX_IDENTITY_BYTES {
                return invalid("a rule's key is empty or too long");
            }
            if rule.display.is_empty() || rule.display.len() > MAX_DISPLAY_BYTES {
                return invalid("a rule's display is empty or too long");
            }
            if self.rules[..index]
                .iter()
                .any(|prior| prior.id == rule.id || prior.key == rule.key)
            {
                return invalid("an id or a key appears twice");
            }
        }
        Ok(())
    }

    /// The rule for `key`, if there is one (`ruleForKey`, `:563-566`).
    pub fn rule_for_key(&self, key: &RuleKey) -> Option<&Rule> {
        self.rules.iter().find(|rule| &rule.key == key)
    }

    /// The rule with this id, if there is one (`ruleForId`, `:556-561`).
    pub fn rule_for_id(&self, id: u64) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.id == id)
    }

    /// What the store says about `key` (`decide`, `:568-574`).
    pub fn decide(&self, key: &RuleKey) -> Verdict {
        match self.rule_for_key(key) {
            Some(rule) if rule.decision == Decision::Allow => Verdict::Allow,
            Some(_) => Verdict::Deny,
            None => Verdict::Unresolved,
        }
    }

    /// `apply` (`:383-393`, `applySet` `:395-447`, `applyRevoke` `:477-496`).
    ///
    /// Every change takes one generation: a set rewrites its rule at the
    /// current generation (a new rule takes it as its id too), and a revoke
    /// spends one so a later confirmation of the revoked rule is stale.
    pub fn apply(&self, event: &Event) -> Applied {
        if self.validate().is_err() {
            return Applied::Invalid;
        }
        let Some(next_generation) = self.next_generation.checked_add(1) else {
            return Applied::Invalid;
        };
        match event {
            Event::Set {
                key,
                display,
                decision,
                expected,
            } => {
                if display.is_empty()
                    || display.len() > MAX_DISPLAY_BYTES
                    || key.canonical.is_empty()
                    || key.canonical.len() > MAX_IDENTITY_BYTES
                {
                    return Applied::Invalid;
                }
                let mut next = self.clone();
                if let Some(rule) = next.rules.iter_mut().find(|rule| &rule.key == key) {
                    if *expected != Some(rule.generation) {
                        return Applied::Stale;
                    }
                    rule.display = display.clone();
                    rule.decision = *decision;
                    rule.generation = self.next_generation;
                    next.next_generation = next_generation;
                    return Applied::Applied(next);
                }
                if expected.is_some() {
                    return Applied::Stale;
                }
                if self.rules.len() >= MAX_RULES {
                    return Applied::Full;
                }
                next.rules.push(Rule {
                    id: self.next_generation,
                    key: key.clone(),
                    display: display.clone(),
                    decision: *decision,
                    generation: self.next_generation,
                });
                next.next_generation = next_generation;
                Applied::Applied(next)
            }
            Event::Revoke { id, expected } => {
                let Some(index) = self.rules.iter().position(|rule| rule.id == *id) else {
                    return Applied::Stale;
                };
                if self.rules[index].generation != *expected {
                    return Applied::Stale;
                }
                let mut next = self.clone();
                next.rules.remove(index);
                next.next_generation = next_generation;
                Applied::Applied(next)
            }
        }
    }

    /// The file's bytes (`writePermissionState`, `session_codec.zig:995-1016`).
    pub fn encode(&self) -> Vec<u8> {
        let rules: Vec<Value> = self
            .rules
            .iter()
            .map(|rule| {
                json!({
                    "id": rule.id,
                    "kind": rule.key.kind.label(),
                    "canonical": durable_bytes(&rule.key.canonical),
                    "display_identity": durable_bytes(rule.display.as_bytes()),
                    "decision": rule.decision.label(),
                    "generation": rule.generation,
                })
            })
            .collect();
        let document = json!({
            "schema_version": SCHEMA_VERSION,
            "next_generation": self.next_generation,
            "rules": rules,
        });
        serde_json::to_vec(&document).expect("a rule store is always serializable")
    }

    /// Reads the file's bytes (`decodePermissionState` and
    /// `parsePermissionState`, `session_codec.zig:388-406`, `:1018-1091`):
    /// exactly upstream's keys, this schema, and every invariant
    /// [`Self::validate`] checks.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let invalid = |why: &str| format!("the saved-session permission rules are invalid: {why}");
        if bytes.is_empty() || bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("the file is empty or too large"));
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|err| invalid(&err.to_string()))?;
        let root = exact_object(&value, &["schema_version", "next_generation", "rules"])
            .ok_or_else(|| invalid("the file is not upstream's object"))?;
        if root["schema_version"].as_u64() != Some(SCHEMA_VERSION) {
            return Err(invalid("the schema version is not 2"));
        }
        let next_generation = root["next_generation"]
            .as_u64()
            .ok_or_else(|| invalid("next_generation is not a number"))?;
        let entries = root["rules"]
            .as_array()
            .ok_or_else(|| invalid("rules is not a list"))?;
        if entries.len() > MAX_RULES {
            return Err(invalid("there are more rules than a session may keep"));
        }
        let mut rules = Vec::with_capacity(entries.len());
        for entry in entries {
            let rule = exact_object(
                entry,
                &[
                    "id",
                    "kind",
                    "canonical",
                    "display_identity",
                    "decision",
                    "generation",
                ],
            )
            .ok_or_else(|| invalid("a rule is not upstream's object"))?;
            let kind = rule["kind"]
                .as_str()
                .and_then(KeyKind::parse)
                .ok_or_else(|| invalid("a rule's kind is unknown"))?;
            let canonical = parse_durable_bytes(&rule["canonical"])
                .ok_or_else(|| invalid("a rule's key is not bytes"))?;
            let key = RuleKey::new(kind, canonical)
                .ok_or_else(|| invalid("a rule's key is empty or too long"))?;
            let display = parse_durable_bytes(&rule["display_identity"])
                .ok_or_else(|| invalid("a rule's display is not bytes"))?;
            let decision = rule["decision"]
                .as_str()
                .and_then(Decision::parse)
                .ok_or_else(|| invalid("a rule's decision is unknown"))?;
            rules.push(Rule {
                id: rule["id"]
                    .as_u64()
                    .ok_or_else(|| invalid("a rule's id is not a number"))?,
                key,
                display: String::from_utf8_lossy(&display).into_owned(),
                decision,
                generation: rule["generation"]
                    .as_u64()
                    .ok_or_else(|| invalid("a rule's generation is not a number"))?,
            });
        }
        let state = Self {
            next_generation,
            rules,
        };
        state.validate()?;
        Ok(state)
    }
}

/// An object with exactly `keys` (`exactObject`).
fn exact_object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)))
        .then_some(object)
}

/// `writeDurableBytes` (`session_codec.zig:467-475`): a JSON string when the
/// bytes are UTF-8, else `{"encoding":"base64","data":…}`.
fn durable_bytes(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(text) => Value::String(text.to_string()),
        Err(_) => json!({"encoding": "base64", "data": base64_encode(bytes)}),
    }
}

/// `parseDurableBytes` (`session_codec.zig:477-505`).
fn parse_durable_bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        Value::Object(object) => {
            if object.len() != 2 || object.get("encoding")?.as_str()? != "base64" {
                return None;
            }
            base64_decode(object.get("data")?.as_str()?)
        }
        _ => None,
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding, as `std.base64.standard` writes it.
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let word =
            (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(BASE64[((word >> (18 - 6 * index)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The inverse of [`base64_encode`]; `None` for anything that is not
/// canonical padded standard base64.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |byte: u8| BASE64.iter().position(|&b| b == byte).map(|at| at as u32);
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index == bytes.len() / 4 - 1;
        let padding = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut word = 0u32;
        for &byte in &chunk[..4 - padding] {
            word = (word << 6) | value(byte)?;
        }
        word <<= 6 * padding as u32;
        let decoded = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    (base64_encode(&out) == text).then_some(out)
}

// ---------------------------------------------------------------------------
// the file
// ---------------------------------------------------------------------------

/// Where a session keeps its rules.
pub fn path_in(session_dir: &Path) -> PathBuf {
    session_dir.join(FILE_NAME)
}

/// Writes `state` as the store of the session whose new directory is
/// `session_dir`, before anything else is in it: the rules confirmed while
/// the session was held in memory, as upstream's first turn writes the
/// permission state it held (`hold`, `writeFirstTurn`,
/// `src/core/session_manager/session.zig:415-429`, `:503-539@481104fd`).
/// Through the same staged, owner-only file and rename as [`update`]; no
/// lock, since nothing else can know the directory yet.
pub fn seed(session_dir: &Path, state: &State) -> io::Result<()> {
    write(session_dir, &path_in(session_dir), &state.encode())
}

/// The store at `path`: empty when there is no file, an error naming why when
/// the file cannot be read, is not a regular file, or does not hold a valid
/// store -- which a decision treats as a refusal, as upstream's failed
/// snapshot fails the call.
pub fn load(path: &Path) -> Result<State, String> {
    let file = match open_regular(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(State::default()),
        Err(err) => {
            return Err(format!(
                "the saved-session permission rules cannot be read: {err}"
            ))
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("the saved-session permission rules cannot be read: {err}"))?;
    State::decode(&bytes)
}

/// `path` opened for reading only if it is a regular file.
///
/// `O_NONBLOCK`, so a FIFO put at the store's path opens at once rather than
/// holding the thread until a writer comes -- the TUI reads the store on the
/// thread that holds the terminal -- and `O_NOCTTY`, so a terminal there
/// never becomes this process's. Then judged by its own `fstat`, so what is
/// refused is what was opened, and made blocking again (`src/image.rs`'s
/// `open_regular` opens a vision source the same way).
#[cfg(unix)]
fn open_regular(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(not_regular());
    }
    let flags = rustix::fs::fcntl_getfl(&file)?;
    rustix::fs::fcntl_setfl(&file, flags & !rustix::fs::OFlags::NONBLOCK)?;
    Ok(file)
}

/// Without `O_NONBLOCK`, the path's own metadata is what keeps a special
/// file from being opened.
#[cfg(not(unix))]
fn open_regular(path: &Path) -> io::Result<fs::File> {
    if !fs::metadata(path)?.is_file() {
        return Err(not_regular());
    }
    fs::File::open(path)
}

fn not_regular() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a regular file")
}

/// What the store's file at `path` is now, from its metadata alone: absent,
/// or which object it is with which size and modification time
/// ([`super::FileIdentity`]), or that the metadata could not be read.
///
/// A reader that keeps a view of the store -- the TUI's bare `/permissions`
/// panel -- compares stamps to know the file moved under it, replaced,
/// written, removed or broken, at the cost of one `stat` rather than a read
/// and a decode each time it looks. [`update`] replaces the file by a rename,
/// so each change it makes is another object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    Missing,
    File(super::FileIdentity),
    /// Not absent, and not a regular file whose metadata reads -- a `stat`
    /// that failed, a FIFO, a directory: a [`load`] says why. It says nothing
    /// about whether the file held still, so two of them are not "unchanged".
    Unknown,
}

/// The [`Stamp`] of the file at `path` now.
pub fn stamp(path: &Path) -> Stamp {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            Stamp::File(super::FileIdentity::from_metadata(&metadata))
        }
        Ok(_) => Stamp::Unknown,
        Err(err) if err.kind() == io::ErrorKind::NotFound => Stamp::Missing,
        Err(_) => Stamp::Unknown,
    }
}

/// Applies `event` to the store at `path` as one transaction: under the
/// session's `permissions.lock`, the file read again, the event applied to
/// what it holds now, and the result -- when applied -- written through a
/// staged, owner-only file and a rename. The compare-and-swap is against the
/// file, not against what the confirmation was shown: that is what makes a
/// second process's change since then `stale` rather than lost.
pub fn update(path: &Path, event: &Event) -> Result<Applied, String> {
    let dir = path
        .parent()
        .ok_or_else(|| "the saved-session permission rules have no directory".to_string())?;
    let _lock = lock(dir)
        .map_err(|err| format!("the saved-session permission rules cannot be locked: {err}"))?;
    let current = load(path)?;
    let applied = current.apply(event);
    if let Applied::Applied(next) = &applied {
        write(dir, path, &next.encode()).map_err(|err| {
            format!("the saved-session permission rules cannot be written: {err}")
        })?;
    }
    Ok(applied)
}

#[cfg(unix)]
fn lock(dir: &Path) -> io::Result<fs::File> {
    use rustix::fs::{flock, FlockOperation};
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(LOCK_NAME))?;
    // A confirmation is one keystroke; a holder past this is another process
    // that is stuck, and a failure says so rather than waiting for it.
    let started = std::time::Instant::now();
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(file),
            Err(rustix::io::Errno::WOULDBLOCK)
                if started.elapsed() < std::time::Duration::from_secs(2) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(err) => return Err(io::Error::from(err)),
        }
    }
}

#[cfg(not(unix))]
fn lock(dir: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(LOCK_NAME))
}

fn write(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let stage = dir.join(format!(
        ".{FILE_NAME}.{}.{}.staged",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let result = (|| {
        let mut file = options.open(&stage)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&stage, path)?;
        fs::File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage);
    }
    result
}

/// The 16 hex digits upstream's displays carry for a digest
/// (`bytesToHex(hash[0..8])`, `tool_admission.zig:2348-2369`).
pub fn short_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> RuleKey {
        RuleKey::new(KeyKind::Command, text.as_bytes().to_vec()).unwrap()
    }

    fn set(text: &str, decision: Decision, expected: Option<u64>) -> Event {
        Event::Set {
            key: key(text),
            display: format!("shown {text}"),
            decision,
            expected,
        }
    }

    fn applied(result: Applied) -> State {
        match result {
            Applied::Applied(state) => state,
            other => panic!("not applied: {other:?}"),
        }
    }

    #[test]
    fn a_new_rule_takes_the_next_generation_as_its_id() {
        // `rule insertion creates a stable nonzero id` (`:577-597`).
        let empty = State::default();
        let one = applied(empty.apply(&set("git status", Decision::Deny, None)));
        assert_eq!(
            empty.rules.len(),
            0,
            "the store it was applied to is unchanged"
        );
        assert_eq!(one.rules.len(), 1);
        assert_eq!((one.rules[0].id, one.rules[0].generation), (1, 1));
        assert_eq!(one.next_generation, 2);
        assert_eq!(one.decide(&key("git status")), Verdict::Deny);
        assert_eq!(one.decide(&key("git push")), Verdict::Unresolved);
        assert_eq!(one.rule_for_id(1).map(|rule| rule.id), Some(1));
    }

    #[test]
    fn a_replacement_keeps_the_id_and_needs_the_generation_it_was_shown() {
        // `rule replacement preserves id and prior state` (`:599-635`).
        let one = applied(State::default().apply(&set("ls", Decision::Allow, None)));
        let two = applied(one.apply(&set("ls", Decision::Deny, Some(1))));
        assert_eq!(two.rules[0].id, 1);
        assert_eq!(two.rules[0].generation, 2);
        assert_eq!(two.decide(&key("ls")), Verdict::Deny);
        // Shown generation 1, but the rule is at 2 now: stale.
        assert_eq!(
            two.apply(&set("ls", Decision::Allow, Some(1))),
            Applied::Stale
        );
        // Shown no rule, but there is one: stale.
        assert_eq!(two.apply(&set("ls", Decision::Allow, None)), Applied::Stale);
        // Shown a rule that is not there: stale.
        assert_eq!(
            State::default().apply(&set("ls", Decision::Allow, Some(1))),
            Applied::Stale
        );
    }

    #[test]
    fn a_revoke_needs_the_id_and_the_generation_and_spends_one() {
        let one = applied(State::default().apply(&set("ls", Decision::Allow, None)));
        assert_eq!(
            one.apply(&Event::Revoke { id: 9, expected: 1 }),
            Applied::Stale
        );
        assert_eq!(
            one.apply(&Event::Revoke { id: 1, expected: 2 }),
            Applied::Stale
        );
        let gone = applied(one.apply(&Event::Revoke { id: 1, expected: 1 }));
        assert!(gone.rules.is_empty());
        assert_eq!(gone.next_generation, 3);
        // A rule made after it does not reuse the id.
        let again = applied(gone.apply(&set("ls", Decision::Allow, None)));
        assert_eq!(again.rules[0].id, 3);
    }

    #[test]
    fn the_store_is_full_at_its_cap_and_invalid_when_its_invariants_break() {
        let mut full = State::default();
        for index in 0..MAX_RULES as u64 {
            full.rules.push(Rule {
                id: index + 1,
                key: key(&format!("c{index}")),
                display: "d".to_string(),
                decision: Decision::Allow,
                generation: index + 1,
            });
        }
        full.next_generation = MAX_RULES as u64 + 1;
        assert_eq!(
            full.apply(&set("one more", Decision::Deny, None)),
            Applied::Full
        );
        let mut broken = State::default();
        broken.rules.push(Rule {
            id: 5,
            key: key("x"),
            display: "d".to_string(),
            decision: Decision::Allow,
            generation: 5,
        });
        assert_eq!(
            broken.apply(&set("y", Decision::Allow, None)),
            Applied::Invalid
        );
        let empty_display = Event::Set {
            key: key("z"),
            display: String::new(),
            decision: Decision::Allow,
            expected: None,
        };
        assert_eq!(State::default().apply(&empty_display), Applied::Invalid);
    }

    #[test]
    fn the_file_is_upstreams_shape_and_reads_back_whole() {
        let mut state = applied(State::default().apply(&Event::Set {
            key: RuleKey::new(KeyKind::FileMutation, vec![0, 159, 146, 150]).unwrap(),
            display: "write_file b.txt preimage=absent after=a1fce4363854ff88".to_string(),
            decision: Decision::Deny,
            expected: None,
        }));
        state = applied(state.apply(&set("ls", Decision::Allow, None)));
        let text = String::from_utf8(state.encode()).unwrap();
        assert_eq!(
            text,
            "{\"schema_version\":2,\"next_generation\":3,\"rules\":[{\"id\":1,\"kind\":\"file_mutation\",\"canonical\":{\"encoding\":\"base64\",\"data\":\"AJ+Slg==\"},\"display_identity\":\"write_file b.txt preimage=absent after=a1fce4363854ff88\",\"decision\":\"deny\",\"generation\":1},{\"id\":2,\"kind\":\"command\",\"canonical\":\"ls\",\"display_identity\":\"shown ls\",\"decision\":\"allow\",\"generation\":2}]}"
        );
        assert_eq!(State::decode(text.as_bytes()), Ok(state));
        // Upstream's empty store after a revoke (`cap/fx-permissions3-…-files`).
        assert_eq!(
            State::decode(br#"{"schema_version":2,"next_generation":3,"rules":[]}"#),
            Ok(State {
                next_generation: 3,
                rules: Vec::new()
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_store_that_is_no_regular_file_is_refused_without_waiting_for_a_writer() {
        // A FIFO at the store's path: every decision and the TUI's panel read
        // the store, the panel on the thread that holds the terminal, so the
        // open must not wait for a writer. A directory is no store either.
        // The read runs on its own thread, bounded, so a regression fails
        // rather than hangs.
        let dir = tempfile::tempdir().expect("a session directory");
        let fifo = path_in(dir.path());
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo failed: {status:?}");
        assert_eq!(stamp(&fifo), Stamp::Unknown);
        let (tx, rx) = std::sync::mpsc::channel();
        let target = fifo.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load(&target));
        });
        let loaded = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(loaded) => loaded,
            Err(_) => {
                drop(fs::OpenOptions::new().write(true).open(&fifo));
                panic!("reading a FIFO store waited for a writer");
            }
        };
        let refused = loaded.expect_err("a FIFO read as a store");
        assert!(refused.contains("not a regular file"), "{refused}");
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).expect("a directory");
        assert_eq!(stamp(&folder), Stamp::Unknown);
        assert!(load(&folder).is_err());
    }

    #[test]
    fn a_file_that_is_not_exactly_a_valid_store_is_refused() {
        for bad in [
            &b""[..],
            b"[]",
            b"{\"schema_version\":1,\"next_generation\":1,\"rules\":[]}",
            b"{\"schema_version\":2,\"next_generation\":0,\"rules\":[]}",
            b"{\"schema_version\":2,\"next_generation\":1,\"rules\":[],\"extra\":1}",
            b"{\"schema_version\":2,\"next_generation\":2,\"rules\":[{\"id\":1,\"kind\":\"command\",\"canonical\":\"x\",\"display_identity\":\"d\",\"decision\":\"maybe\",\"generation\":1}]}",
            b"{\"schema_version\":2,\"next_generation\":2,\"rules\":[{\"id\":2,\"kind\":\"command\",\"canonical\":\"x\",\"display_identity\":\"d\",\"decision\":\"allow\",\"generation\":1}]}",
            b"{\"schema_version\":2,\"next_generation\":3,\"rules\":[{\"id\":1,\"kind\":\"command\",\"canonical\":\"x\",\"display_identity\":\"d\",\"decision\":\"allow\",\"generation\":1},{\"id\":2,\"kind\":\"command\",\"canonical\":\"x\",\"display_identity\":\"d\",\"decision\":\"deny\",\"generation\":2}]}",
            b"{\"schema_version\":2,\"next_generation\":2,\"rules\":[{\"id\":1,\"kind\":\"command\",\"canonical\":{\"encoding\":\"base64\",\"data\":\"!!\"},\"display_identity\":\"d\",\"decision\":\"allow\",\"generation\":1}]}",
        ] {
            assert!(State::decode(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn base64_round_trips_and_refuses_what_is_not_canonical() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"\xff\x00\x80"] {
            assert_eq!(base64_decode(&base64_encode(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        for bad in ["Zm9vYg=", "Zm9=vYg=", "Zm9vYh==", "Zm9v Yg=="] {
            assert_eq!(base64_decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_change_to_the_file_is_a_compare_and_swap_against_what_the_file_holds() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(dir.path());
        assert_eq!(load(&path), Ok(State::default()));
        let first = update(&path, &set("ls", Decision::Allow, None)).unwrap();
        assert!(matches!(first, Applied::Applied(_)));
        // Another confirmation shown no rule loses to the one that landed.
        assert_eq!(
            update(&path, &set("ls", Decision::Deny, None)).unwrap(),
            Applied::Stale
        );
        assert_eq!(load(&path).unwrap().decide(&key("ls")), Verdict::Allow);
        assert!(matches!(
            update(&path, &Event::Revoke { id: 1, expected: 1 }).unwrap(),
            Applied::Applied(_)
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"schema_version\":2,\"next_generation\":3,\"rules\":[]}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // A file that does not read is an error, and nothing is written over it.
        fs::write(&path, "{broken").unwrap();
        assert!(load(&path).is_err());
        assert!(update(&path, &set("x", Decision::Deny, None)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
    }

    #[test]
    fn a_command_key_is_the_command_its_directory_and_the_platform() {
        let one = RuleKey::command("ls", Path::new("/w"), "macos").unwrap();
        let mut expected = Vec::new();
        for part in [
            &b"fx-permission-state-v2"[..],
            b"ls",
            b"/w",
            b"foreground",
            b"macos",
        ] {
            expected.extend_from_slice(&(part.len() as u64).to_be_bytes());
            expected.extend_from_slice(part);
        }
        assert_eq!(one.canonical, expected);
        assert_ne!(
            one,
            RuleKey::command("ls", Path::new("/w/sub"), "macos").unwrap()
        );
        assert_ne!(
            one,
            RuleKey::command("ls ", Path::new("/w"), "macos").unwrap()
        );
        assert_eq!(short_hex(b"x").len(), 16);
    }
}
