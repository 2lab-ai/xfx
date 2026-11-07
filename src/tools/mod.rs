//! The closed tool registry.
//!
//! The registry is a `static` table of four read-only specs. It is not
//! configurable, not extensible at runtime, and not merged with anything: what
//! xfx advertises to a model is this list, in this order, and the code that runs
//! is the same object that was advertised
//! (`vercel-labs/fx@580a0c5d src/builtins/tools.zig:1351-1380`).
//!
//! Two consequences are the point:
//!
//! - **Advertisement is a promise.** [`ADVERTISED_TOOLS`] is reconciled against
//!   `docs/parity.md` by `scripts/check-no-stubs.sh`, so a tool cannot reach a
//!   model schema before its parity row says `implemented`.
//! - **A call for anything else is not answered.** [`Registry::execute`] returns
//!   [`UnadvertisedTool`] rather than inventing a result, and the turn ends
//!   rather than continuing on a false premise.

pub mod mcp;
pub mod mutate;
pub mod question;
pub mod read;
pub mod skill;
pub mod spec;
pub mod terminal;
pub mod undo;

use std::fmt;

use serde_json::Value;

use crate::gateway::protocol::ToolCall;

pub use question::{QuestionRequester, Reply as QuestionReply};
pub use spec::is_refusal;
pub use spec::{
    Activity, InputSchema, LineChanges, Outcome, PermissionKind, Presentation, Property,
    PropertyKind, RaceInterlude, Shown, ToolContext, ToolDecoder, ToolExecutor, ToolInput,
    ToolLimits, ToolResult, ToolSession, ToolSpec, ToolValidator,
};

/// Every tool name this build advertises, in registry order.
///
/// `scripts/check-no-stubs.sh` reads this declaration textually and requires an
/// `implemented` row in `docs/parity.md` for each name.
pub const ADVERTISED_TOOLS: &[&str] = &[
    "list_files",
    "glob_files",
    "grep_files",
    "read_file",
    "write_file",
    "edit_file",
    "create_folder",
    "terminal",
    "capability_search",
    "skill",
    "mcp_select_tool",
    "ask_user_question",
];

/// The specs themselves, in upstream's order (`tools.zig:1352-1367`): the read
/// group, then the mutation group, then the terminal -- and, where upstream
/// puts them, between its shell and `ask_user_question`, the MCP meta-tools
/// with `skill` among them (`advertisement_order`, `vercel-labs/fx@c1db9191
/// src/builtins/tools.zig:865-881`).
static BUILTIN_TOOLS: &[ToolSpec] = &[
    read::LIST_FILES,
    read::GLOB_FILES,
    read::GREP_FILES,
    read::READ_FILE,
    mutate::WRITE_FILE,
    mutate::EDIT_FILE,
    mutate::CREATE_FOLDER,
    terminal::TERMINAL,
    mcp::CAPABILITY_SEARCH,
    skill::SKILL,
    mcp::MCP_SELECT_TOOL,
    question::ASK_USER_QUESTION,
];

/// A tool call xfx never offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnadvertisedTool {
    pub name: String,
}

impl fmt::Display for UnadvertisedTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a tool this build advertises; the advertised tools are {}",
            self.name,
            ADVERTISED_TOOLS.join(", ")
        )
    }
}

impl std::error::Error for UnadvertisedTool {}

/// One exact action, prepared and not run, as a saved-session rule names it:
/// the key the rule is kept under and the line that shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAction {
    pub key: crate::permission::session_rules::RuleKey,
    pub display: String,
}

/// Prepares the call `name(arguments)` exactly as a model's call would be
/// prepared, and runs nothing: what `/permissions remember` keys a rule on
/// (`preparePermissionStateAction`, `vercel-labs/fx@c1db9191
/// src/core/tooling/tool_admission.zig:2257-2302`).
///
/// The tools a rule can be about are the ones a decision is made for: every
/// one -- the three changes, the command, the four reads and the tools judged
/// by their name ([`JUDGED_BY_NAME`]). A change is refused when a
/// configured rule denies it (upstream's `PermissionStatePreparationDenied`)
/// -- which the caller's session must be in `ask` to see, as upstream's
/// preflight is, since full access consults no table -- and a change that
/// would do nothing -- a folder already there -- has nothing to prepare. A
/// read, or a tool judged by its name, is prepared as upstream prepares a
/// structured call: its key and its label, with no rule consulted
/// (`:2262-2267`).
/// Every refusal is a sentence for the log; the person is told upstream's one
/// line ([`crate::permission::slash::NOT_PREPARED`]).
pub fn prepare_rule_action(
    name: &str,
    arguments: &Value,
    context: &ToolContext,
) -> Result<RuleAction, String> {
    use crate::permission::{PolicyDecision, ProposedAction};
    let spec = Registry::builtin()
        .spec(name)
        .ok_or_else(|| format!("`{name}` is not a tool"))?;
    let input = spec.decode(arguments)?;
    let prepared_change = match &input {
        ToolInput::WriteFile(input) => {
            mutate::prepare_write_file(input, context, mutate::Proof::NotRequired)
        }
        ToolInput::EditFile(input) => {
            mutate::prepare_edit_file(input, context, mutate::Proof::NotRequired)
        }
        ToolInput::CreateFolder(input) => mutate::prepare_create_folder(input, context),
        ToolInput::Terminal(input) => {
            let plan = crate::permission::CommandPlan::prepare(
                &input.command,
                context.scope(),
                input.cwd.as_deref(),
                context.limits(),
            )?;
            let action = ProposedAction::Command(&plan);
            let key = action
                .state_key()
                .ok_or_else(|| "the command has no key".to_string())?;
            return Ok(RuleAction {
                key,
                display: action.state_display(),
            });
        }
        ToolInput::ReadFile(_)
        | ToolInput::ListFiles(_)
        | ToolInput::GlobFiles(_)
        | ToolInput::GrepFiles(_) => {
            let plan = read::prepare_read(&input, context)?;
            let action = ProposedAction::Read(&plan);
            let key = action
                .state_key()
                .ok_or_else(|| "the read has no key".to_string())?;
            return Ok(RuleAction {
                key,
                display: action.state_display(),
            });
        }
        // A tool judged by its name is prepared as upstream prepares every
        // call that is neither a change nor a command: its question's label
        // and its key, no rule consulted (`preparePermissionStateAction`,
        // `vercel-labs/fx@c1db9191 src/core/tooling/tool_admission.zig:
        // 2262-2267`). The key is the one [`run_judged_by_name`] decides by.
        ToolInput::AskUserQuestion(_)
        | ToolInput::CapabilitySearch(_)
        | ToolInput::Skill(_)
        | ToolInput::McpSelectTool(_) => {
            let plan = crate::permission::ToolCallPlan {
                tool: spec.name(),
                targets: targets_of(spec.name(), arguments, context),
                label: label_of(spec.name(), arguments),
                arguments: arguments.to_string(),
            };
            let action = ProposedAction::Tool(&plan);
            let key = action
                .state_key()
                .ok_or_else(|| "the call has no key".to_string())?;
            return Ok(RuleAction {
                key,
                display: action.state_display(),
            });
        }
    };
    let prepared = prepared_change.map_err(|result| result.output)?;
    let action = ProposedAction::Mutation(&prepared.plan);
    if let PolicyDecision::Deny {
        cause: crate::permission::DenyCause::ConfiguredRule,
        reason,
    } = context.permissions().evaluate(action)
    {
        return Err(reason);
    }
    let key = action
        .state_key()
        .ok_or_else(|| "the change has no key".to_string())?;
    Ok(RuleAction {
        key,
        display: action.state_display(),
    })
}

/// The closed set of tools a turn may run.
#[derive(Debug, Clone, Copy)]
pub struct Registry {
    specs: &'static [ToolSpec],
}

impl Registry {
    /// The one registry this build has.
    pub const fn builtin() -> Self {
        Self {
            specs: BUILTIN_TOOLS,
        }
    }

    pub fn specs(&self) -> &'static [ToolSpec] {
        self.specs
    }

    /// The advertised names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.specs.iter().map(ToolSpec::name).collect()
    }

    pub fn spec(&self, name: &str) -> Option<&'static ToolSpec> {
        self.specs.iter().find(|spec| spec.name() == name)
    }

    /// The `tools` array of a Gateway request: one closed schema per spec.
    pub fn advertisement(&self) -> Vec<Value> {
        self.specs.iter().map(ToolSpec::advertisement).collect()
    }

    /// The specs a request made in `mode` under the configured `rules`
    /// offers the model, in registry order: every one but those the table
    /// refuses at every target -- and every one in full access, which
    /// consults no table (`appendBuiltinTool`, `vercel-labs/fx@c1db9191
    /// src/core/tooling/tool_projection.zig:563-566`) -- and, where no MCP
    /// client runs (`mcp` false), neither of the two MCP meta-tools.
    ///
    /// Leaving a tool out is not what keeps it from running -- a call to it
    /// anyway is still judged, and the same table refuses it -- it is what
    /// keeps the model from spending a step on a call that can only be
    /// refused.
    pub fn offered(
        &self,
        mode: crate::permission::PermissionMode,
        rules: &crate::permission::RuleSet,
        mcp: bool,
    ) -> Vec<&'static ToolSpec> {
        self.specs
            .iter()
            .filter(|spec| mcp || !mcp::META_TOOLS.contains(&spec.name()))
            .filter(|spec| {
                mode == crate::permission::PermissionMode::Yolo
                    || !rules.denies_every_target_of_tool(upstream_name(spec.name()))
            })
            .collect()
    }

    /// [`Self::offered`]'s schemas, as a request's `tools` array.
    pub fn advertisement_under(
        &self,
        mode: crate::permission::PermissionMode,
        rules: &crate::permission::RuleSet,
        mcp: bool,
    ) -> Vec<Value> {
        self.offered(mode, rules, mcp)
            .into_iter()
            .map(ToolSpec::advertisement)
            .collect()
    }

    /// What a step running under `context` advertises, decided now: the
    /// registry as [`Self::offered`] leaves it, then every MCP tool this
    /// turn has selected that the table in force does not refuse
    /// ([`mcp::offered_selected`]), in the order it was selected
    /// (`cap/fx-mcp-wire-40x120-bodies/003-completion.json`). A turn takes
    /// the registry's part once, when it begins, and this second part at
    /// every step (`crate::agent::machine`).
    pub fn advertisement_for(&self, context: &ToolContext) -> Vec<Value> {
        // The session is let go before the selected tools are read: that
        // reading takes it again, and it is not reentrant.
        let (mode, rules) = {
            let permissions = context.permissions();
            (permissions.mode(), permissions.rules())
        };
        let mut tools = self.advertisement_under(mode, &rules, context.mcp().is_some());
        tools.extend(mcp::offered_selected(context));
        tools
    }

    /// Runs one model tool call.
    ///
    /// Ok(result) covers both "it worked" and "it refused": a refusal is
    /// something the model can act on, so it travels back as a correlated tool
    /// result. `Err` is reserved for the one case the turn cannot represent --
    /// a tool that was never offered.
    ///
    /// Permission admission is *not* here. It is inside the executors that need
    /// it, because a decision has to be made about a prepared plan -- an exact
    /// target with its exact preimage, an exact argv with its exact cwd -- and
    /// only the executor can produce one. A gate at this level would have to
    /// decide from the raw arguments, which is the mistake this design exists to
    /// avoid: it would judge a path the model wrote rather than the file that
    /// path currently resolves to.
    ///
    /// What is asserted here instead is that every [`PermissionKind`] that
    /// requires an authority belongs to a spec whose executor mints one; see
    /// `every_mutating_spec_goes_through_a_permission_decision`.
    ///
    /// The one exception is a tool with nothing to prepare -- no file, no
    /// command: [`JUDGED_BY_NAME`]. Its plan is its name and its arguments,
    /// which this level has, so it is judged here ([`run_judged_by_name`]).
    pub fn execute(
        &self,
        call: &ToolCall,
        context: &ToolContext,
    ) -> Result<ToolResult, UnadvertisedTool> {
        let Some(spec) = self.spec(&call.name) else {
            return Err(UnadvertisedTool {
                name: call.name.clone(),
            });
        };
        let mut result = if JUDGED_BY_NAME.contains(&spec.name()) {
            run_judged_by_name(spec, call, context)
        } else {
            spec.run(&call.input, context)
        };
        // A backstop, not the bound. Each executor caps its own output; this
        // guarantees the property for all of them in one place, and says so
        // rather than truncating quietly.
        if let Some(clipped) = spec::clip(&result.output, context.limits().max_output_bytes) {
            result.output = format!(
                "{clipped}\n... [tool output truncated at {} bytes]",
                context.limits().max_output_bytes
            );
        }
        Ok(result)
    }
}

/// The tools that name no file and run no command, which upstream judges
/// against the table under their own name: every tool it registers has a
/// permission contract (`toolHasPermissionContract`, `vercel-labs/fx@c1db9191
/// src/core/tooling/tool_admission.zig:101-103`), and one whose target kind is
/// `none` is matched at its own name (`permissions.zig:226`). So `*: deny`, or
/// a deny on the name, refuses the call at admission; a rule that asks puts
/// it to the person first, in auto too (`:1389-1442`); and with no rule it
/// runs in every mode, needing no approval of its own (`:1123-1125`).
///
/// Every other spec is judged by its own executor, against the plan it
/// prepared; `every_spec_is_judged_somewhere` keeps the two lists whole.
///
/// `skill` is one of them too, judged under `skill` at the skill it loads
/// rather than at its own name ([`targets_of`]).
pub const JUDGED_BY_NAME: &[&str] = &[
    "capability_search",
    "skill",
    "mcp_select_tool",
    "ask_user_question",
];

/// Decodes `call`, judges it under its tool's own name, and runs it only if
/// the session admits it. Decoded first, so a malformed call is refused for
/// what it is and nobody is asked about it.
fn run_judged_by_name(spec: &ToolSpec, call: &ToolCall, context: &ToolContext) -> ToolResult {
    use crate::permission::{Decided, PolicyDecision, ProposedAction, ToolCallPlan};
    if let Err(reason) = spec.decode(&call.input) {
        return ToolResult::failure(reason);
    }
    let plan = ToolCallPlan {
        tool: spec.name(),
        targets: targets_of(spec.name(), &call.input, context),
        label: label_of(spec.name(), &call.input),
        arguments: call.input.to_string(),
    };
    // Its own statement, so the session is not held while the tool runs:
    // the MCP meta-tools read its table themselves.
    let Decided { decision, feedback } = context
        .permissions()
        .decide_with_feedback(ProposedAction::Tool(&plan));
    let result = match decision {
        PolicyDecision::Allow { .. } => spec.run(&call.input, context),
        PolicyDecision::Deny { reason, .. } => ToolResult::refused(spec.name(), &reason),
        // `decide_with_feedback` resolves every question; this arm keeps a
        // future decision from being read as a yes.
        PolicyDecision::Prompt => {
            ToolResult::refused(spec.name(), "the approval was never resolved")
        }
    };
    match feedback {
        Some(_) => result.with_feedback(feedback),
        None => result,
    }
}

/// The argument upstream's question shows after a [`JUDGED_BY_NAME`] tool
/// (`label_arg_kind`, `vercel-labs/fx@c1db9191 src/builtins/tools.zig`:
/// `.query`, `.name`, and `.none` for `ask_user_question`;
/// `labelValueForKind`, `src/core/tooling/tool_dispatch.zig:597-614`), or
/// nothing when the call has none.
fn label_of(tool: &str, input: &Value) -> String {
    let field = match tool {
        "capability_search" => "query",
        "skill" => "name",
        "mcp_select_tool" => "name",
        _ => return String::new(),
    };
    input
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Where a [`JUDGED_BY_NAME`] call is judged: at the tool's own name
/// (`permissions.zig:226`), and a `skill` call at the skill it loads, which
/// upstream names by the skill the location resolves to, else the call's
/// `name` (`permissionTargetForCall`, `vercel-labs/fx@c1db9191
/// src/core/permissions/permissions.zig:181-185`) -- never at the resource
/// it reads, so a deny on a skill refuses every way of loading any part of it
/// (`resolved skill calls retain name policy`, `tool_admission.zig:
/// 3840-3870`).
///
/// Every name the call could be about is judged, the resolved skill's first:
/// the catalog's name for the location, the `name` the call gave, and the
/// location's last component, the skill's name unless its directory was
/// renamed. A deny at any of them refuses, so a call that names a denied
/// skill some other way is still refused; one that names nothing is judged
/// at `skill`.
fn targets_of(tool: &'static str, input: &Value, context: &ToolContext) -> Vec<String> {
    if tool != "skill" {
        return vec![tool.to_string()];
    }
    let mut targets: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !name.is_empty() && !targets.iter().any(|known| known == name) {
            targets.push(name.to_string());
        }
    };
    let location = input.get("location").and_then(Value::as_str);
    if let (Some(location), Some(catalog)) = (location, context.skills()) {
        if let Ok(path) = catalog.resolve(location) {
            if let Some(skill) = catalog.skills.iter().find(|skill| skill.path == path) {
                push(&skill.name);
            }
        }
    }
    if let Some(name) = input.get("name").and_then(Value::as_str) {
        push(name);
    }
    if let Some(location) = location {
        if let Some(leaf) = location.rsplit('/').next() {
            push(leaf);
        }
    }
    if targets.is_empty() {
        targets.push(tool.to_string());
    }
    targets
}

/// The name upstream's projection judges a spec by.
///
/// `terminal` is upstream's `shell` -- the one tool the two products ship
/// under different names -- and is judged as upstream judges `shell`: under
/// that name alone, which `permissionNameForTool` maps to no category
/// (`vercel-labs/fx@c1db9191 src/core/permissions/permissions.zig:1455-1464`).
/// So `shell: deny` or `*: deny` leaves it out of the request and `bash:
/// deny`, which refuses every command it would run, does not -- what the same
/// settings file does upstream. Every other spec is judged by its own name,
/// under the category a call to it is judged under
/// ([`crate::permission::rules::permission_name_for_tool`]).
fn upstream_name(name: &'static str) -> &'static str {
    match name {
        "terminal" => "shell",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::AccessScope;
    use serde_json::json;

    fn context() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        (dir, ToolContext::new(scope))
    }

    #[test]
    fn the_eight_original_schemas_are_byte_identical() {
        // Captured at the tip before the array extension. A byte that moves here is
        // a change to what every model already sees. `read_file`'s moved once,
        // on purpose: it reads images now, and says so in upstream's words
        // (`read_file_description`, `builtins/tools.zig:42-43`).
        const PINNED: &[(&str, &str)] = &[
            (
                "list_files",
                r#"{"description":"List the entries of one directory, one level deep, without reading file contents. Paths are relative to the workspace root, or absolute inside an authorized root; anything else is refused. Directories end in /, symlinks in @. Entries named .git, node_modules, dist, build, coverage, .next, zig-out, or .zig-cache are always omitted; everything else, including dotfiles, is listed. When to use: inspect a known folder, confirm a name, or choose the next path to read. When NOT to use: recursive discovery, content search, or a shell ls.","inputSchema":{"additionalProperties":false,"properties":{"path":{"description":"Directory to list. Defaults to the workspace root.","type":"string"}},"type":"object"},"name":"list_files","type":"function"}"#,
            ),
            (
                "glob_files",
                r#"{"description":"Find file paths matching a glob pattern below one directory, with mode=count for an exact count without listing. Paths are relative to the workspace root, or absolute inside an authorized root. The search does not see everything: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, and hidden dot-paths are skipped unless the pattern itself names one (for example .github/**/*.yml). Results are sorted, and a capped or incomplete search says so on its own ... line. When to use: locate files by name, extension, or directory shape. When NOT to use: search file contents, read a file, or count non-file things.","inputSchema":{"additionalProperties":false,"properties":{"mode":{"description":"Use matches to list paths, or count for an exact count without listing.","enum":["matches","count"],"type":"string"},"path":{"description":"Directory to search below. Defaults to the workspace root.","type":"string"},"pattern":{"description":"Glob pattern to match, such as src/**/*.rs or *.md.","type":"string"}},"required":["pattern"],"type":"object"},"name":"glob_files","type":"function"}"#,
            ),
            (
                "grep_files",
                r#"{"description":"Search text files for a literal substring, optionally narrowed by path and include glob, with modes for matching lines, files with matches, or counts, plus head_limit/offset paging and bounded context_lines. Paths are relative to the workspace root, or absolute inside an authorized root. Regular expressions are not supported: the pattern is matched literally. The search does not see every file: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, hidden dot-paths are skipped unless the include glob itself names one, and files that are not UTF-8 text or are above the size cap are not searched. Any file skipped for the last two reasons is counted on a ... skipped line, so no matches means no matches among the files actually searched. When to use: find an exact symbol, string, or usage site. When NOT to use: filename lookup, reading a known path, or regex search.","inputSchema":{"additionalProperties":false,"properties":{"case_insensitive":{"description":"Match without regard to case.","type":"boolean"},"context_lines":{"description":"Lines to show before and after each match. Bounded by the tool.","type":"integer"},"head_limit":{"description":"Positive maximum results to return. Defaults to the output cap.","type":"integer"},"include":{"description":"Glob applied to candidate paths before any file is read, such as *.rs.","type":"string"},"mode":{"description":"Use matches for lines, files_with_matches for paths, or count for exact counts.","enum":["matches","files_with_matches","count"],"type":"string"},"offset":{"description":"Zero-based result offset for paging. Defaults to 0.","type":"integer"},"path":{"description":"Directory to search below. Defaults to the workspace root.","type":"string"},"pattern":{"description":"Literal substring to search for. Not a regular expression.","type":"string"}},"required":["pattern"],"type":"object"},"name":"grep_files","type":"function"}"#,
            ),
            (
                "read_file",
                r#"{"description":"Read one UTF-8 text file as bounded, line-numbered output, with an optional start_line/line_count range; image files (PNG, JPEG, GIF, WebP up to 3.9MB) attach to the result so you can see them. Paths are relative to the workspace root, or absolute inside an authorized root. Output states how many of the file's lines it showed, so a partial read is never mistaken for the whole file. When to use: inspect an exact known path, or view an image file. When NOT to use: list a directory, search many files, or read non-image binary data.","inputSchema":{"additionalProperties":false,"properties":{"line_count":{"description":"Positive number of lines to return. Defaults to the read cap.","type":"integer"},"path":{"description":"Path relative to the workspace root, or an absolute path inside an authorized root.","type":"string"},"start_line":{"description":"1-based first line to return. Defaults to 1.","type":"integer"}},"required":["path"],"type":"object"},"name":"read_file","type":"function"}"#,
            ),
            (
                "write_file",
                r#"{"description":"Create a file, or replace an existing file's entire contents. An existing file must have been read in full with read_file first, and must not have changed since that read; otherwise the call is refused so that unseen content is never discarded. The replacement is staged in the same directory and renamed into place, so a reader never sees a half-written file, and the previous permission bits are preserved. When to use: add a new file, or intentionally rewrite a small or generated one. When NOT to use: a focused change to an existing file (use edit_file), creating directories (use create_folder), or deleting anything.","inputSchema":{"additionalProperties":false,"properties":{"content":{"description":"The complete new contents of the file.","type":"string"},"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path","content"],"type":"object"},"name":"write_file","type":"function"}"#,
            ),
            (
                "edit_file",
                r#"{"description":"Replace exactly one occurrence of old_string with new_string in an existing UTF-8 text file. The file must have been read in full with read_file first and must not have changed since. old_string must appear exactly once: if it appears zero times or more than once the call is refused rather than guessing, so include enough surrounding text to make it unique. An edit whose result equals the current contents changes nothing and says so. When to use: a focused patch after reading the file. When NOT to use: whole-file rewrites (use write_file), ambiguous repeated text, or files you have not read.","inputSchema":{"additionalProperties":false,"properties":{"new_string":{"description":"The text to put in its place.","type":"string"},"old_string":{"description":"The exact text to replace. Must occur exactly once in the file.","type":"string"},"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path","old_string","new_string"],"type":"object"},"name":"edit_file","type":"function"}"#,
            ),
            (
                "create_folder",
                r#"{"description":"Create a directory, including any missing parent directories. Existing directories are left alone and reported as already present. When to use: prepare a location for files you are about to write. When NOT to use: create files, inspect a directory (use list_files), or build speculative structure the task did not ask for.","inputSchema":{"additionalProperties":false,"properties":{"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path"],"type":"object"},"name":"create_folder","type":"function"}"#,
            ),
            (
                "terminal",
                r#"{"description":"Run one command in the workspace and return its captured result: exit status, standard output, and standard error. Set action to exec. A recognized read-only command runs as an exact argument list with no shell, so quoting, globbing, variable substitution, redirection, and operators such as |, &&, ;, and > are not expanded and take the command off the automatic route. Commands that compile or run project code always need approval even though the automatic mode may have written the files they would compile; this includes cargo test, build, check, clippy, bench, run, and fmt, because a cargo alias in .cargo/config.toml can redirect any subcommand that is not a cargo built-in. The automatic cargo surface is cargo --version, cargo -V, cargo --list, and cargo metadata --no-deps. Operands must be relative, must not contain .., and must not resolve outside the authorized roots. Anything else needs an explicit approval before it runs. Output is captured, not streamed, and is truncated past a fixed size; the command is killed if it outruns its time limit. There is no sandbox: an approved command runs with the invoking user's privileges. When to use: build, test, lint, or inspect version control state. When NOT to use: reading or searching files (use the file tools), long-lived or interactive processes, or anything that publishes, installs, deletes, or reaches the network.","inputSchema":{"additionalProperties":false,"properties":{"action":{"description":"The only supported action is exec: run one command and capture its result.","enum":["exec"],"type":"string"},"command":{"description":"The command to run, as one line.","type":"string"},"cwd":{"description":"Directory to run in, inside an authorized root. Defaults to the workspace root.","type":"string"}},"required":["action","command"],"type":"object"},"name":"terminal","type":"function"}"#,
            ),
        ];
        let registry = Registry::builtin();
        assert_eq!(PINNED.len(), 8);
        for (name, expected) in PINNED {
            let spec = registry.spec(name).expect("an advertised tool");
            assert_eq!(
                &serde_json::to_string(&spec.advertisement()).unwrap(),
                expected,
                "`{name}`'s advertised schema moved"
            );
        }
    }

    #[test]
    fn the_declared_inventory_is_the_registry() {
        // The textual inventory `scripts/check-no-stubs.sh` reads and the table
        // the product actually runs cannot drift apart.
        assert_eq!(Registry::builtin().names(), ADVERTISED_TOOLS);
    }

    /// What a request offers under `rows`, with an MCP client running.
    fn offered(
        mode: crate::permission::PermissionMode,
        rows: &[(&str, &str, crate::permission::RuleAction)],
    ) -> Vec<&'static str> {
        offered_with(mode, rows, true)
    }

    fn offered_with(
        mode: crate::permission::PermissionMode,
        rows: &[(&str, &str, crate::permission::RuleAction)],
        mcp: bool,
    ) -> Vec<&'static str> {
        let rules = crate::permission::RuleSet::new(
            rows.iter()
                .map(|(permission, pattern, action)| {
                    crate::permission::ConfiguredRule::new(*permission, *pattern, *action)
                })
                .collect(),
        );
        Registry::builtin()
            .offered(mode, &rules, mcp)
            .into_iter()
            .map(ToolSpec::name)
            .collect()
    }

    #[test]
    fn a_tool_a_global_deny_covers_is_left_out_of_the_request_and_full_access_offers_everything() {
        use crate::permission::{PermissionMode, RuleAction::*};
        // Upstream's category-wide deny takes both edit tools out
        // (`tool_projection.zig:719-743`); xfx's `create_folder` is an `edit`
        // too.
        assert_eq!(
            offered(PermissionMode::Ask, &[("edit", "*", Deny)]),
            [
                "list_files",
                "glob_files",
                "grep_files",
                "read_file",
                "terminal",
                "capability_search",
                "skill",
                "mcp_select_tool",
                "ask_user_question"
            ]
        );
        assert_eq!(
            offered(PermissionMode::Auto, &[("read", "*", Deny)]),
            [
                "list_files",
                "glob_files",
                "grep_files",
                "write_file",
                "edit_file",
                "create_folder",
                "terminal",
                "capability_search",
                "skill",
                "mcp_select_tool",
                "ask_user_question"
            ]
        );
        // `list_files` is judged as `glob`, as its calls are.
        assert!(!offered(PermissionMode::Ask, &[("glob", "*", Deny)]).contains(&"list_files"));
        // Not a global deny: a narrower one, an ask, a later way through.
        assert_eq!(
            offered(PermissionMode::Ask, &[("edit", "src/*", Deny)]),
            ADVERTISED_TOOLS
        );
        assert_eq!(
            offered(PermissionMode::Ask, &[("edit", "*", Ask)]),
            ADVERTISED_TOOLS
        );
        assert_eq!(
            offered(
                PermissionMode::Ask,
                &[("edit", "*", Deny), ("edit", "src/*", Ask)]
            ),
            ADVERTISED_TOOLS
        );
        // Everything denied: nothing offered, and full access offers it all
        // (`yolo advertisement ignores permission filtering`, `:705-717`).
        assert!(offered(PermissionMode::Ask, &[("*", "*", Deny)]).is_empty());
        assert_eq!(
            offered(PermissionMode::Yolo, &[("*", "*", Deny)]),
            ADVERTISED_TOOLS
        );
    }

    #[test]
    fn every_spec_is_judged_somewhere() {
        // Upstream judges every tool it registers (`toolHasPermissionContract`,
        // `tool_admission.zig:101-103`). Each spec here is judged either by
        // its executor, against the plan it prepares, or by its name in
        // `Registry::execute`; a new spec fails here until someone says which.
        for spec in Registry::builtin().specs() {
            let by_name = JUDGED_BY_NAME.contains(&spec.name());
            let by_executor = match spec.name() {
                "list_files" | "glob_files" | "grep_files" | "read_file" => true,
                "write_file" | "edit_file" | "create_folder" => true,
                "terminal" => true,
                name if by_name => {
                    assert!(
                        !spec.permission().requires_authority(),
                        "{name} mints an authority, so it has a plan to judge"
                    );
                    false
                }
                other => panic!("{other} is judged nowhere"),
            };
            assert!(by_name != by_executor, "{}", spec.name());
        }
    }

    #[test]
    fn a_tool_judged_by_its_name_is_refused_by_a_rule_that_covers_it_and_runs_without_one() {
        use crate::permission::{
            ConfiguredRule, PermissionMode, PermissionSession, RuleAction, RuleSet,
        };
        let run = |mode: PermissionMode, rows: &[(&str, &str, RuleAction)], name: &str| {
            let dir = tempfile::tempdir().expect("temporary workspace");
            let scope = AccessScope::primary_only(dir.path()).expect("scope");
            let rules = RuleSet::new(
                rows.iter()
                    .map(|(permission, pattern, action)| {
                        ConfiguredRule::new(*permission, *pattern, *action)
                    })
                    .collect(),
            );
            // `ask` with nobody to ask, so a question anybody would have
            // had to answer is a refusal here.
            let context = ToolContext::new(scope)
                .with_permissions(PermissionSession::new(mode).with_rules(rules));
            let input = match name {
                "capability_search" => json!({ "query": "anything" }),
                "mcp_select_tool" => json!({ "name": "mcp_some_tool" }),
                "skill" => json!({ "location": "/skills/some-skill" }),
                _ => {
                    json!({ "questions": [{ "question": "q", "options": [{ "label": "a" }, { "label": "b" }] }] })
                }
            };
            let call = ToolCall {
                id: "c1".into(),
                name: name.into(),
                input,
            };
            Registry::builtin()
                .execute(&call, &context)
                .expect("a registered tool")
        };
        use RuleAction::*;
        for name in JUDGED_BY_NAME {
            // A deny that covers it -- everything, or its own name -- refuses
            // it at admission, in every mode but full access.
            for mode in [PermissionMode::Ask, PermissionMode::Auto] {
                for rows in [&[("*", "*", Deny)][..], &[(*name, "*", Deny)][..]] {
                    let refused = run(mode, rows, name);
                    assert_eq!(refused.outcome, Outcome::Denied, "{name} {mode:?} {rows:?}");
                    assert!(
                        refused
                            .output
                            .contains(&format!("a configured permission rule denies `{name}`")),
                        "{name}: {}",
                        refused.output
                    );
                }
            }
            // A rule that asks: nobody to ask, so refused, naming the rule --
            // in auto too.
            let asked = run(PermissionMode::Auto, &[(name, "*", Ask)], name);
            assert_eq!(asked.outcome, Outcome::Denied, "{name}");
            assert!(
                asked.output.contains(&format!(
                    "a configured permission rule asks before `{name}`"
                )),
                "{name}: {}",
                asked.output
            );
            // Not a refusal: no rule, a narrower one, an allow, full access.
            for (mode, rows) in [
                (PermissionMode::Ask, &[][..]),
                (PermissionMode::Ask, &[("edit", "*", Deny)][..]),
                (
                    PermissionMode::Ask,
                    &[("*", "*", Deny), (*name, "*", Allow)][..],
                ),
                (PermissionMode::Yolo, &[("*", "*", Deny)][..]),
            ] {
                let ran = run(mode, rows, name);
                assert_ne!(
                    ran.outcome,
                    Outcome::Denied,
                    "{name} {mode:?} {rows:?}: {}",
                    ran.output
                );
            }
        }
    }

    #[test]
    fn a_deny_on_a_skill_refuses_every_way_of_loading_any_part_of_it() {
        // Upstream judges a skill call at the skill, whatever part of it the
        // call reads (`permissionTargetForCall`, `permissions.zig:181-185`;
        // `resolved skill calls retain name policy`, `tool_admission.zig:
        // 3840-3870`): its canonical form with a resource, its legacy named
        // form, and a location alone are all refused by a deny on its name.
        use crate::permission::{PermissionMode, PermissionSession, RuleSet};
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        let rules =
            RuleSet::parse(&json!({ "skill": { "restricted-name": "deny" } })).expect("rules");
        for mode in [PermissionMode::Ask, PermissionMode::Auto] {
            let context = ToolContext::new(scope.clone())
                .with_permissions(PermissionSession::new(mode).with_rules(rules.clone()));
            for input in [
                json!({ "location": "/installed/restricted-name", "resource": "reference.md" }),
                json!({ "location": "/installed/restricted-name" }),
                json!({ "name": "restricted-name", "location": "/installed/renamed", "offset": 1 }),
            ] {
                let call = ToolCall {
                    id: "c1".into(),
                    name: "skill".into(),
                    input: input.clone(),
                };
                let result = Registry::builtin()
                    .execute(&call, &context)
                    .expect("registered");
                assert_eq!(
                    result.outcome,
                    Outcome::Denied,
                    "{mode:?} {input}: {}",
                    result.output
                );
                assert!(
                    result.output.contains(
                        "a configured permission rule denies `skill` for `restricted-name`"
                    ),
                    "{}",
                    result.output
                );
            }
            // Another skill is not covered.
            let other = ToolCall {
                id: "c2".into(),
                name: "skill".into(),
                input: json!({ "location": "/installed/other-skill" }),
            };
            let result = Registry::builtin()
                .execute(&other, &context)
                .expect("registered");
            assert_ne!(result.outcome, Outcome::Denied, "{}", result.output);
        }
    }

    #[test]
    fn a_malformed_call_of_a_tool_judged_by_name_is_refused_for_what_it_is() {
        use crate::permission::{PermissionMode, PermissionSession};
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        // `ask` with nobody to ask: a call that reached a question would be
        // refused as one. This one never gets that far.
        let context = ToolContext::new(scope).with_permissions(
            PermissionSession::new(PermissionMode::Ask).with_rules(
                crate::permission::RuleSet::parse(&json!({ "*": "ask" })).expect("rules"),
            ),
        );
        let call = ToolCall {
            id: "c1".into(),
            name: "capability_search".into(),
            input: json!({}),
        };
        let result = Registry::builtin()
            .execute(&call, &context)
            .expect("registered");
        assert_ne!(result.outcome, Outcome::Denied, "{}", result.output);
        assert!(!result.output.contains("asks before"), "{}", result.output);
    }

    #[test]
    fn an_mcp_meta_tool_is_left_out_by_a_deny_on_its_name_and_where_no_mcp_client_runs() {
        use crate::permission::{PermissionMode, RuleAction::*};
        // Upstream projects its two MCP meta-tools as it projects every
        // builtin, so a deny on a meta-tool's own name -- a category of its
        // own (`permissionNameForTool`, `permissions.zig:1455-1464`) -- takes
        // it out (`tool_projection.zig:563-566`), and full access puts it back.
        for name in mcp::META_TOOLS {
            let denied = offered(PermissionMode::Auto, &[(name, "*", Deny)]);
            assert!(!denied.contains(name), "{name}: {denied:?}");
            assert_eq!(denied.len(), ADVERTISED_TOOLS.len() - 1, "{name}");
            assert!(offered(PermissionMode::Yolo, &[(name, "*", Deny)]).contains(name));
            // An ask is not a refusal.
            assert!(offered(PermissionMode::Ask, &[(name, "*", Ask)]).contains(name));
        }
        // `mcp_*` covers the one named so and nothing else.
        let globbed = offered(PermissionMode::Ask, &[("mcp_*", "*", Deny)]);
        assert!(!globbed.contains(&"mcp_select_tool"));
        assert!(globbed.contains(&"capability_search"));
        // No MCP client: neither of the two, whatever the table says.
        let none = offered_with(PermissionMode::Yolo, &[], false);
        assert!(mcp::META_TOOLS.iter().all(|name| !none.contains(name)));
        assert_eq!(none.len(), ADVERTISED_TOOLS.len() - mcp::META_TOOLS.len());
    }

    #[test]
    fn the_terminal_is_left_out_as_upstreams_shell_is_and_not_by_a_rule_about_commands() {
        use crate::permission::{PermissionMode, RuleAction::*};
        assert!(!offered(PermissionMode::Ask, &[("shell", "*", Deny)]).contains(&"terminal"));
        // `bash` refuses every command the terminal would run, and upstream
        // still advertises `shell` under it; so does xfx, and the call is
        // refused when it comes.
        assert!(offered(PermissionMode::Ask, &[("bash", "*", Deny)]).contains(&"terminal"));
        assert!(offered(PermissionMode::Ask, &[("terminal", "*", Deny)]).contains(&"terminal"));
    }

    #[test]
    fn every_spec_declares_the_authority_its_effects_need() {
        // The registry's contract in one table: reading is free, changing a file
        // or starting a process is not. A new spec that forgets to declare its
        // kind fails here rather than at a user's expense.
        let expected = [
            ("list_files", PermissionKind::ReadOnly),
            ("glob_files", PermissionKind::ReadOnly),
            ("grep_files", PermissionKind::ReadOnly),
            ("read_file", PermissionKind::ReadOnly),
            ("write_file", PermissionKind::MutateFile),
            ("edit_file", PermissionKind::MutateFile),
            ("create_folder", PermissionKind::MutateFile),
            ("terminal", PermissionKind::RunCommand),
            ("capability_search", PermissionKind::ReadOnly),
            ("skill", PermissionKind::ReadOnly),
            ("mcp_select_tool", PermissionKind::ReadOnly),
            ("ask_user_question", PermissionKind::Interaction),
        ];
        let actual: Vec<(&str, PermissionKind)> = Registry::builtin()
            .specs()
            .iter()
            .map(|spec| (spec.name(), spec.permission()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_mutating_spec_goes_through_a_permission_decision() {
        // A context whose session is `ask` with no approval channel can admit
        // nothing. Every spec that declares it needs an authority must therefore
        // refuse, which is what proves the executor consults policy rather than
        // declaring a kind and ignoring it.
        let (_dir, context) = context();
        let arguments = [
            ("write_file", json!({ "path": "a.txt", "content": "x" })),
            (
                "edit_file",
                json!({ "path": "a.txt", "old_string": "a", "new_string": "b" }),
            ),
            ("create_folder", json!({ "path": "made" })),
            ("terminal", json!({ "action": "exec", "command": "pwd" })),
        ];
        for (name, input) in arguments {
            let spec = Registry::builtin().spec(name).expect("advertised");
            assert!(spec.permission().requires_authority(), "{name}");
            let result = Registry::builtin()
                .execute(
                    &ToolCall {
                        id: "c1".to_string(),
                        name: name.to_string(),
                        input,
                    },
                    &context,
                )
                .expect("the tool is advertised");
            assert!(!result.ok, "{name} ran without an authority: {result:?}");
        }
    }

    #[test]
    fn no_two_specs_share_a_name() {
        let mut names = Registry::builtin().names();
        names.sort();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique);
    }

    #[test]
    fn every_required_field_is_a_declared_property() {
        for spec in Registry::builtin().specs() {
            let schema = spec.input_schema();
            for required in schema.required {
                assert!(
                    schema
                        .properties
                        .iter()
                        .any(|property| property.name == *required),
                    "{} requires an undeclared `{required}`",
                    spec.name()
                );
            }
        }
    }

    #[test]
    fn a_call_for_an_unadvertised_tool_is_not_answered() {
        let (_dir, context) = context();
        let err = Registry::builtin()
            .execute(
                &ToolCall {
                    id: "c1".to_string(),
                    // Genuinely deferred: `delete_file` is an upstream tool
                    // with a `deferred` parity row, so this stays a real case
                    // rather than one that expires the moment a tool lands.
                    name: "delete_file".to_string(),
                    input: json!({}),
                },
                &context,
            )
            .expect_err("delete_file is not advertised");
        assert_eq!(err.name, "delete_file");
        let message = err.to_string();
        assert!(message.contains("delete_file"), "{message}");
        assert!(message.contains("read_file"), "{message}");
    }

    #[test]
    fn no_advertised_schema_nests_deeper_than_two_array_edges() {
        // `depth` counts **object levels**, so the question tool's own schema --
        // outer object, question object, option object -- is 3, which is two array
        // edges. The bound is on the edges; naming it 2 while counting objects
        // would fail on the very schema this task exists to allow.
        fn depth(schema: InputSchema) -> usize {
            1 + schema
                .properties
                .iter()
                .filter_map(|property| property.array)
                .map(|array| depth(*array.items))
                .max()
                .unwrap_or(0)
        }
        for spec in Registry::builtin().specs() {
            assert!(
                depth(spec.input_schema()) <= 3,
                "`{}` nests too deep",
                spec.name()
            );
        }
        // And the eight scalar-only tools stay flat, so the bound is not vacuous.
        for name in ["list_files", "read_file", "write_file", "terminal"] {
            assert_eq!(
                depth(Registry::builtin().spec(name).unwrap().input_schema()),
                1
            );
        }
    }

    #[test]
    fn an_oversized_output_is_truncated_with_a_sentence_that_says_so() {
        // `list_files` bounds its entry *count*, not its byte count, so a
        // directory of long names is exactly the case the registry's backstop
        // exists for.
        let dir = tempfile::tempdir().expect("temporary workspace");
        for index in 0..20 {
            std::fs::write(dir.path().join(format!("entry-{index:02}.txt")), "x").expect("write");
        }
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        let context = ToolContext::with_limits(
            scope,
            ToolLimits {
                max_output_bytes: 60,
                ..ToolLimits::default()
            },
        );
        let result = Registry::builtin()
            .execute(
                &ToolCall {
                    id: "c1".to_string(),
                    name: "list_files".to_string(),
                    input: json!({}),
                },
                &context,
            )
            .expect("list_files is advertised");
        assert!(result.ok);
        assert!(
            result
                .output
                .contains("[tool output truncated at 60 bytes]"),
            "{}",
            result.output
        );
        // The kept prefix is bounded; only the sentence that explains it is not.
        let (kept, _) = result
            .output
            .split_once("\n... [tool output truncated")
            .expect("the sentinel is on its own line");
        assert!(kept.len() <= 60, "{}", kept.len());
    }
}
