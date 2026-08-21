//! What a tool *is*: an immutable specification plus the bounds it runs under.
//!
//! A [`ToolSpec`] is a `const` value. It owns its name, its description, its
//! closed JSON schema, its decoder, its validator, its permission kind, and its
//! executor, and none of them can be replaced at runtime. That is deliberate:
//! the schema xfx advertises and the code that runs when the model uses it are
//! the same object, so they cannot drift apart, and no caller can add a tool the
//! parity ledger has not accounted for
//! (`vercel-labs/fx@580a0c5d src/builtins/tools.zig:509-627`).
//!
//! Execution is staged -- decode, validate, admit, execute -- and each stage can
//! only refuse or hand a *more* specific value to the next one. A decoder cannot
//! read the filesystem and an executor never sees raw JSON, so "the model sent
//! nonsense" and "the filesystem said no" are different failures with different
//! messages (`src/core/tooling/tool_dispatch.zig:122-168`).

use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Map, Value};

use crate::gateway::CancelToken;
use crate::permission::{PermissionSession, ReadTracker};
use crate::workspace::AccessScope;

use super::mutate::{CreateFolderInput, EditFileInput, WriteFileInput};
use super::read::{GlobFilesInput, GrepFilesInput, ListFilesInput, ReadFileInput};
use super::terminal::TerminalInput;

/// How much authority a tool needs before it may run.
///
/// The kind is declared on the spec rather than derived from the arguments, so
/// "is this call dangerous" is answered by the tool's identity and cannot be
/// changed by what the model sent
/// (`vercel-labs/fx@580a0c5d src/core/permissions/permission_gate.zig:59-70`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    /// The tool observes the filesystem and changes nothing. Admitted in every
    /// permission mode: `ask` requires an approval for mutations and commands,
    /// not for reads (design, "Permissions").
    ReadOnly,
    /// The tool changes a file or a directory. Every call mints an authority.
    MutateFile,
    /// The tool starts a process. Every call mints an authority.
    RunCommand,
}

impl PermissionKind {
    /// Whether a call of this kind must cross a permission decision.
    pub fn requires_authority(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

/// The scalar types a tool input field may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    String,
    Integer,
    Boolean,
}

impl PropertyKind {
    fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
        }
    }
}

/// One field of a tool's input schema.
#[derive(Debug, Clone, Copy)]
pub struct Property {
    pub name: &'static str,
    pub kind: PropertyKind,
    pub description: &'static str,
    /// The complete set of accepted values, when the field is an enumeration.
    pub allowed: &'static [&'static str],
}

/// A tool's input schema: a closed object of typed scalars.
///
/// Closed is the point. `additionalProperties: false` is written for every
/// tool, so a field the model invents is rejected by the provider instead of
/// being silently dropped by xfx, and the model learns its mistake in the same
/// turn.
#[derive(Debug, Clone, Copy)]
pub struct InputSchema {
    pub properties: &'static [Property],
    pub required: &'static [&'static str],
}

impl InputSchema {
    /// The JSON Schema fragment sent as `inputSchema`.
    pub fn to_json(self) -> Value {
        let mut properties = Map::new();
        for property in self.properties {
            let mut rendered = Map::new();
            rendered.insert("type".to_string(), Value::from(property.kind.label()));
            rendered.insert("description".to_string(), Value::from(property.description));
            if !property.allowed.is_empty() {
                rendered.insert("enum".to_string(), Value::from(property.allowed.to_vec()));
            }
            properties.insert(property.name.to_string(), Value::Object(rendered));
        }

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::from("object"));
        schema.insert("properties".to_string(), Value::Object(properties));
        schema.insert("additionalProperties".to_string(), Value::Bool(false));
        if !self.required.is_empty() {
            schema.insert("required".to_string(), Value::from(self.required.to_vec()));
        }
        Value::Object(schema)
    }
}

/// A decoded, typed tool input.
///
/// The set is closed and matches the registry exactly, so dispatch is a `match`
/// rather than a downcast: a spec whose decoder and executor disagree about the
/// input type would not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolInput {
    ListFiles(ListFilesInput),
    GlobFiles(GlobFilesInput),
    GrepFiles(GrepFilesInput),
    ReadFile(ReadFileInput),
    WriteFile(WriteFileInput),
    EditFile(EditFileInput),
    CreateFolder(CreateFolderInput),
    Terminal(TerminalInput),
}

/// Turns raw model JSON into a typed input, or says why it cannot.
pub type ToolDecoder = fn(&Value) -> Result<ToolInput, String>;

/// Checks a decoded input without touching the filesystem.
pub type ToolValidator = fn(&ToolInput) -> Result<(), String>;

/// Runs a validated input against a scope and returns a model-visible result.
pub type ToolExecutor = fn(&ToolInput, &ToolContext) -> ToolResult;

/// What one tool call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Whether the tool did what it was asked.
    ///
    /// A refusal is a real result, not an error: the model is told exactly what
    /// went wrong and can correct itself in the next step.
    pub ok: bool,
    /// The text the model sees.
    pub output: String,
    /// A one-line summary for the `tool_result` event a human watches.
    pub detail: String,
    /// Whether this result ends the turn instead of going back to the model.
    ///
    /// Almost always false: a refusal is information the model can act on. It is
    /// true for exactly one situation -- an authority that was granted and then
    /// stopped describing the world, because the file moved underneath it. That
    /// is not an argument the model can fix, and letting it retry would let
    /// whoever won the race keep racing.
    pub fatal: bool,
}

impl ToolResult {
    /// A completed call. The summary is bounded like a refusal's, so no result
    /// can flood the event stream a human is watching.
    pub fn success(output: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
            detail: summarize(&detail.into()),
            fatal: false,
        }
    }

    /// A refusal. The summary is the first line of the explanation, bounded, so
    /// a long filesystem error cannot flood a human's event stream.
    pub fn failure(output: impl Into<String>) -> Self {
        let output = output.into();
        let detail = summarize(&output);
        Self {
            ok: false,
            output,
            detail,
            fatal: false,
        }
    }

    /// A refusal that ends the turn: the authority for this call stopped being
    /// true before it could be spent.
    pub fn revoked(output: impl Into<String>) -> Self {
        Self {
            fatal: true,
            ..Self::failure(output)
        }
    }
}

/// The first line of `text`, at most 160 bytes, on a character boundary.
fn summarize(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    match clip(line, 160) {
        Some(clipped) => format!("{clipped}..."),
        None => line.to_string(),
    }
}

/// `text` clipped to at most `limit` bytes, or `None` when it already fits.
pub(crate) fn clip(text: &str, limit: usize) -> Option<&str> {
    if text.len() <= limit {
        return None;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(&text[..end])
}

/// The fixed ceilings every read tool runs under.
///
/// They exist so one tool call cannot consume a turn's context window or its
/// wall clock. Each default is upstream's; the field exists so a later slice can
/// lower one without editing four executors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolLimits {
    /// Directory entries `list_files` shows (`tool_dispatch.zig:46`).
    pub max_list_entries: usize,
    /// Lines `read_file` shows, whether or not the model names a count
    /// (`tool_dispatch.zig:48`).
    pub max_read_lines: usize,
    /// Bytes of one line `read_file` shows before clipping it
    /// (`tool_dispatch.zig:49`).
    pub max_read_line_len: usize,
    /// Bytes of a file `read_file` will load (`read_file.zig:14`).
    pub max_read_bytes: usize,
    /// Bytes of one tool result the model may receive (`read_file.zig:16`).
    pub max_output_bytes: usize,
    /// Paths `glob_files` lists (`tool_dispatch.zig:46`).
    pub max_glob_matches: usize,
    /// Matches `grep_files` renders (`src/core/workspace/grep_search.zig:14`).
    pub max_grep_matches: usize,
    /// Matches `grep_files` collects before it stops scanning
    /// (`grep_search.zig:17`).
    pub max_grep_scan: usize,
    /// Bytes of one file `grep_files` will search
    /// (`src/tools/filesystem/grep_files.zig:14`).
    pub max_grep_file_bytes: usize,
    /// Lines of context `grep_files` will show around a match
    /// (`grep_files.zig:13`).
    pub max_context_lines: usize,
    /// Files a walk will consider before it reports itself incomplete
    /// (`src/core/workspace/workspace_files.zig:10`).
    pub max_candidates: usize,
    /// Bytes a single mutation may write
    /// (`src/tools/filesystem/edit_file.zig:6`).
    pub max_mutation_bytes: usize,
    /// Bytes of one command's captured output, per stream
    /// (`src/core/permissions/direct_command.zig:10`).
    pub max_command_output_bytes: usize,
    /// How long one command may run before it is killed.
    ///
    /// An xfx value. Upstream's foreground executor takes an optional timeout
    /// and its callers supply their own (`sandbox.zig:152`); a coding agent that
    /// appears to hang is indistinguishable from a broken one, so xfx always has
    /// a ceiling.
    pub command_timeout_ms: u64,
}

impl Default for ToolLimits {
    fn default() -> Self {
        Self {
            max_list_entries: 100,
            max_read_lines: 400,
            max_read_line_len: 2_000,
            max_read_bytes: 10 * 1024 * 1024,
            max_output_bytes: 256 * 1024,
            max_glob_matches: 100,
            max_grep_matches: 200,
            max_grep_scan: 2_000,
            max_grep_file_bytes: 200 * 1024,
            max_context_lines: 5,
            max_candidates: 100_000,
            max_mutation_bytes: 4 * 1024 * 1024,
            max_command_output_bytes: 64 * 1024,
            command_timeout_ms: 120_000,
        }
    }
}

/// A callback run at the exact moment an authority is minted and not yet spent.
///
/// This is the race window, and it is the only part of the mutation path that
/// cannot be observed from outside the process. A test installs a closure here
/// to change the filesystem at precisely that instant and prove the stale
/// authority is refused. Nothing in the product installs one: `ToolContext`
/// leaves it `None`, and `app.rs` never sets it.
pub type RaceInterlude = Arc<dyn Fn() + Send + Sync>;

/// The mutable state one run of xfx shares across every tool call.
///
/// Behind an `Arc` so that cloning a [`ToolContext`] -- which the turn does --
/// shares one set of read proofs and one permission ledger rather than forking
/// them. A session grant given during step two has to be visible in step three.
pub struct ToolSession {
    permissions: Mutex<PermissionSession>,
    reads: Mutex<ReadTracker>,
}

impl ToolSession {
    pub fn new(permissions: PermissionSession) -> Self {
        Self {
            permissions: Mutex::new(permissions),
            reads: Mutex::new(ReadTracker::new()),
        }
    }
}

impl std::fmt::Debug for ToolSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSession").finish_non_exhaustive()
    }
}

/// Everything a tool executor is allowed to know.
///
/// It carries no provider and no credential: a tool cannot reach the network or
/// the model even by accident. What it does carry is the authority to change
/// things -- the scope, the bounds, the permission session, and the turn's
/// cancellation flag -- because those are exactly the facts an executor needs
/// and nothing more.
#[derive(Clone)]
pub struct ToolContext {
    scope: AccessScope,
    limits: ToolLimits,
    session: Arc<ToolSession>,
    cancel: CancelToken,
    interlude: Option<RaceInterlude>,
}

impl std::fmt::Debug for ToolContext {
    /// The interlude is a closure with no representation, so its presence is
    /// printed rather than its identity -- and printing it at all is the point:
    /// a context with one installed is a test context.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("scope", &self.scope)
            .field("limits", &self.limits)
            .field("session", &self.session)
            .field("cancelled", &self.cancel.is_cancelled())
            .field("has_race_interlude", &self.interlude.is_some())
            .finish()
    }
}

impl ToolContext {
    /// A context with the shipped limits and the most restrictive session.
    ///
    /// The default session is `ask` with no approval channel, so a context built
    /// without an explicit permission session can read but cannot change
    /// anything. That is the safe direction to be wrong in.
    pub fn new(scope: AccessScope) -> Self {
        Self::with_limits(scope, ToolLimits::default())
    }

    /// A context with explicit limits, so a test can prove a bound without
    /// building a fixture large enough to hit the shipped one.
    pub fn with_limits(scope: AccessScope, limits: ToolLimits) -> Self {
        Self {
            scope,
            limits,
            session: Arc::new(ToolSession::new(PermissionSession::default())),
            cancel: CancelToken::new(),
            interlude: None,
        }
    }

    /// The same context running under `permissions`.
    pub fn with_permissions(mut self, permissions: PermissionSession) -> Self {
        self.session = Arc::new(ToolSession::new(permissions));
        self
    }

    /// The same context, cancelled by `cancel`.
    ///
    /// A long-running command has to stop when the user presses Ctrl-C, and the
    /// turn's token is the one thing that already means that.
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// The same context with a [`RaceInterlude`] installed. Tests only.
    pub fn with_race_interlude(mut self, interlude: RaceInterlude) -> Self {
        self.interlude = Some(interlude);
        self
    }

    pub fn scope(&self) -> &AccessScope {
        &self.scope
    }

    pub fn limits(&self) -> &ToolLimits {
        &self.limits
    }

    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }

    /// The permission session, locked.
    ///
    /// A poisoned lock is recovered rather than propagated: a panic in one tool
    /// executor must not make every later permission decision panic too, and the
    /// session's invariants are "a set of nonces and a list of grants", which a
    /// partial update cannot corrupt.
    pub fn permissions(&self) -> MutexGuard<'_, PermissionSession> {
        self.session
            .permissions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// This session's read proofs, locked.
    pub fn reads(&self) -> MutexGuard<'_, ReadTracker> {
        self.session
            .reads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs the installed interlude, if a test installed one.
    pub(crate) fn run_race_interlude(&self) {
        if let Some(interlude) = &self.interlude {
            interlude();
        }
    }
}

/// One tool, whole: what it is called, what it accepts, and what it does.
#[derive(Clone, Copy)]
pub struct ToolSpec {
    name: &'static str,
    description: &'static str,
    permission: PermissionKind,
    input_schema: InputSchema,
    decode: ToolDecoder,
    validate: ToolValidator,
    execute: ToolExecutor,
}

impl ToolSpec {
    /// Builds a spec. `const` so the registry is a static table rather than
    /// something assembled -- and therefore mutable -- at startup.
    pub const fn new(
        name: &'static str,
        description: &'static str,
        permission: PermissionKind,
        input_schema: InputSchema,
        decode: ToolDecoder,
        validate: ToolValidator,
        execute: ToolExecutor,
    ) -> Self {
        Self {
            name,
            description,
            permission,
            input_schema,
            decode,
            validate,
            execute,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn description(&self) -> &'static str {
        self.description
    }

    pub fn permission(&self) -> PermissionKind {
        self.permission
    }

    pub fn input_schema(&self) -> InputSchema {
        self.input_schema
    }

    /// The Gateway's flattened function-tool envelope
    /// (`vercel-labs/fx@580a0c5d src/core/tooling/gateway_schema.zig:82-104`).
    pub fn advertisement(&self) -> Value {
        let mut envelope = Map::new();
        envelope.insert("type".to_string(), Value::from("function"));
        envelope.insert("name".to_string(), Value::from(self.name));
        envelope.insert("description".to_string(), Value::from(self.description));
        envelope.insert("inputSchema".to_string(), self.input_schema.to_json());
        Value::Object(envelope)
    }

    /// Decodes, validates, and runs one call's arguments.
    ///
    /// The stages are separate so that a refusal names the stage it came from.
    /// Nothing here reaches the filesystem until the input is typed and
    /// self-consistent.
    pub fn run(&self, input: &Value, context: &ToolContext) -> ToolResult {
        let decoded = match (self.decode)(input) {
            Ok(decoded) => decoded,
            Err(reason) => return ToolResult::failure(reason),
        };
        if let Err(reason) = (self.validate)(&decoded) {
            return ToolResult::failure(reason);
        }
        (self.execute)(&decoded, context)
    }
}

impl std::fmt::Debug for ToolSpec {
    /// Function pointers are not worth printing; the identity is the name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSpec")
            .field("name", &self.name)
            .field("permission", &self.permission)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// shared decoding
// ---------------------------------------------------------------------------
//
// Every decoder is strict: a field of the wrong type is a refusal naming the
// field, not a value quietly treated as absent. The schema is closed and typed,
// so "the model sent `path: 7`" is a mistake the model can only fix if it is
// told.

pub(crate) fn object<'a>(tool: &str, input: &'a Value) -> Result<&'a Map<String, Value>, String> {
    input
        .as_object()
        .ok_or_else(|| format!("{tool} arguments must be a JSON object"))
}

pub(crate) fn required_string(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    match object.get(key) {
        None => Err(format!("{tool} requires the string field `{key}`")),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{tool} field `{key}` must be a string")),
    }
}

pub(crate) fn optional_string(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{tool} field `{key}` must be a string")),
    }
}

pub(crate) fn optional_bool(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<bool, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{tool} field `{key}` must be a boolean")),
    }
}

/// An optional integer field with an inclusive lower bound.
pub(crate) fn optional_integer(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
    minimum: u64,
) -> Result<Option<usize>, String> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let described = if minimum == 0 {
        "non-negative"
    } else {
        "positive"
    };
    let number = value
        .as_u64()
        .filter(|number| *number >= minimum)
        .ok_or_else(|| format!("{tool} field `{key}` must be a {described} integer"))?;
    Ok(Some(usize::try_from(number).unwrap_or(usize::MAX)))
}

/// An optional field whose value must be one of `allowed`.
pub(crate) fn optional_enum(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<Option<String>, String> {
    let Some(raw) = optional_string(tool, object, key)? else {
        return Ok(None);
    };
    if allowed.contains(&raw.as_str()) {
        return Ok(Some(raw));
    }
    Err(format!(
        "{tool} field `{key}` must be one of {}",
        allowed.join(", ")
    ))
}

/// Rejects a field that is present but blank once trimmed.
pub(crate) fn nonblank(tool: &str, key: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{tool} field `{key}` must not be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: InputSchema = InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: "A path.",
                allowed: &[],
            },
            Property {
                name: "mode",
                kind: PropertyKind::String,
                description: "A mode.",
                allowed: &["matches", "count"],
            },
        ],
        required: &["path"],
    };

    #[test]
    fn a_schema_is_always_closed() {
        let rendered = SAMPLE.to_json();
        assert_eq!(rendered["additionalProperties"], json!(false));
        assert_eq!(rendered["type"], "object");
        assert_eq!(rendered["required"], json!(["path"]));
        assert_eq!(rendered["properties"]["path"]["type"], "string");
        assert_eq!(
            rendered["properties"]["mode"]["enum"],
            json!(["matches", "count"])
        );
        assert!(rendered["properties"]["path"].get("enum").is_none());
    }

    #[test]
    fn a_wrong_typed_field_is_refused_by_name_rather_than_ignored() {
        let object = json!({ "path": 7, "flag": "yes", "count": -1 });
        let object = object.as_object().unwrap();
        assert_eq!(
            required_string("t", object, "path").unwrap_err(),
            "t field `path` must be a string"
        );
        assert_eq!(
            optional_bool("t", object, "flag").unwrap_err(),
            "t field `flag` must be a boolean"
        );
        assert_eq!(
            optional_integer("t", object, "count", 0).unwrap_err(),
            "t field `count` must be a non-negative integer"
        );
    }

    #[test]
    fn an_absent_or_null_optional_field_is_absent() {
        let object = json!({ "path": null });
        let object = object.as_object().unwrap();
        assert_eq!(optional_string("t", object, "path").unwrap(), None);
        assert_eq!(optional_string("t", object, "missing").unwrap(), None);
        assert_eq!(optional_integer("t", object, "missing", 1).unwrap(), None);
        assert!(!optional_bool("t", object, "missing").unwrap());
    }

    #[test]
    fn a_missing_required_field_names_the_tool_and_the_field() {
        let object = json!({});
        let message =
            required_string("read_file", object.as_object().unwrap(), "path").unwrap_err();
        assert_eq!(message, "read_file requires the string field `path`");
    }

    #[test]
    fn an_enum_field_lists_what_it_would_have_accepted() {
        let object = json!({ "mode": "sideways" });
        let message =
            optional_enum("t", object.as_object().unwrap(), "mode", &["a", "b"]).unwrap_err();
        assert_eq!(message, "t field `mode` must be one of a, b");
    }

    #[test]
    fn clipping_never_splits_a_character() {
        assert_eq!(clip("short", 10), None);
        // Each `한` is three bytes, so a nine-byte clip of a twelve-byte string
        // must land on a boundary rather than mid-character.
        assert_eq!(clip("한한한한", 10), Some("한한한"));
        assert_eq!(clip("abcdef", 3), Some("abc"));
    }

    #[test]
    fn a_failure_summary_is_the_first_line_and_is_bounded() {
        let result = ToolResult::failure("no such path: `x`\nmore detail");
        assert!(!result.ok);
        assert_eq!(result.detail, "no such path: `x`");

        let long = ToolResult::failure("e".repeat(400));
        assert!(long.detail.len() <= 164, "{}", long.detail.len());
        assert!(long.detail.ends_with("..."));
    }
}
