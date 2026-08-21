//! Permission authorities, secure mutations, and terminal execution.
//!
//! Four promises are proven here, and each one is a release promise:
//!
//! 1. **Nothing mutates without an authority.** Decode, validate, prepare,
//!    policy, mint, revalidate, execute are separate stages. A decision cannot
//!    change its own target, an authority is good for exactly one execution, and
//!    an authority whose world moved underneath it is refused.
//! 2. **`ask` fails closed.** With no real approval channel a mutation or a
//!    command is denied, never assumed.
//! 3. **`auto` admits a declared set and nothing else.** Bounded reversible
//!    writes inside the workspace, and a small read/test command grammar that
//!    runs as an argv with no shell. Everything destructive or dynamic is denied
//!    by name.
//! 4. **`yolo` says so.** It skips policy and prints a visible warning.
//!
//! Nothing here uses a real credential. Upstream evidence is pinned to
//! `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

mod support;

use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

use fxr::agent::{run_turn, TurnError, TurnRequest};
use fxr::gateway::protocol::{Completion, CompletionRequest, FinishReason, ToolCall, Usage};
use fxr::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use fxr::output::{Event, RecordingSink};
use fxr::permission::{
    classify, AllowSource, ApprovalAnswer, ApprovalPrompter, ApprovalRequest, AuthorityError,
    CommandEffect, CommandPlan, CommandRoute, DeniedEffect, Grant, PermissionMode, PermissionRules,
    PermissionSession, PolicyDecision, ProposedAction, Rule, YOLO_WARNING,
};
use fxr::tools::{Registry, ToolContext, ToolLimits, ToolResult, ADVERTISED_TOOLS};
use fxr::workspace::AccessScope;

use support::fake_gateway::{finish, sse_body, text_delta, tool_call, FakeGateway, Reply};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "FXR_MODEL",
    "FXR_PERMISSION_MODE",
    "FXR_MAX_AGENT_STEPS",
    "FXR_GATEWAY_URL",
];

/// A test secret that must never appear on stdout, stderr, or in a subprocess.
const TEST_KEY: &str = "fxr-test-permission-key-must-not-appear";

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

struct Tree {
    _dir: TempDir,
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let dir = TempDir::new().expect("create a temporary tree");
        let root = dir.path().canonicalize().expect("canonicalize the tree");
        Self { _dir: dir, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(&path, contents).expect("write the fixture file");
        path
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("read the fixture file")
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(&path).expect("create the fixture directory");
        path
    }

    /// Every entry name directly under `relative`, sorted.
    fn entries(&self, relative: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root.join(relative))
            .expect("read the directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

/// A prompter that answers from a script and records what it was asked.
#[derive(Debug, Default)]
struct PrompterLog {
    requests: Vec<ApprovalRequest>,
}

#[derive(Clone)]
struct ScriptedPrompter {
    answers: Arc<Mutex<VecDeque<Result<ApprovalAnswer, String>>>>,
    log: Arc<Mutex<PrompterLog>>,
}

impl ScriptedPrompter {
    fn new(answers: Vec<ApprovalAnswer>) -> Self {
        Self {
            answers: Arc::new(Mutex::new(answers.into_iter().map(Ok).collect())),
            log: Arc::new(Mutex::new(PrompterLog::default())),
        }
    }

    /// A channel that is present but broken: it errors instead of answering.
    fn broken() -> Self {
        let mut answers: VecDeque<Result<ApprovalAnswer, String>> = VecDeque::new();
        answers.push_back(Err("the approval channel closed".to_string()));
        Self {
            answers: Arc::new(Mutex::new(answers)),
            log: Arc::new(Mutex::new(PrompterLog::default())),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.log
            .lock()
            .expect("log")
            .requests
            .iter()
            .map(|request| format!("{}:{}", request.tool, request.target))
            .collect()
    }

    /// The last question put to the user, whole.
    fn last(&self) -> Option<ApprovalRequest> {
        self.log.lock().expect("log").requests.last().cloned()
    }
}

impl ApprovalPrompter for ScriptedPrompter {
    fn request(&mut self, request: &ApprovalRequest) -> std::io::Result<ApprovalAnswer> {
        self.log.lock().expect("log").requests.push(request.clone());
        match self.answers.lock().expect("answers").pop_front() {
            Some(Ok(answer)) => Ok(answer),
            Some(Err(detail)) => Err(std::io::Error::other(detail)),
            None => Err(std::io::Error::other("the script ran out of answers")),
        }
    }
}

fn session(mode: PermissionMode) -> PermissionSession {
    PermissionSession::new(mode)
}

fn context(tree: &Tree, mode: PermissionMode) -> ToolContext {
    ToolContext::new(AccessScope::primary_only(tree.root()).expect("a usable primary root"))
        .with_permissions(session(mode))
}

fn context_with_limits(tree: &Tree, mode: PermissionMode, limits: ToolLimits) -> ToolContext {
    ToolContext::with_limits(
        AccessScope::primary_only(tree.root()).expect("a usable primary root"),
        limits,
    )
    .with_permissions(session(mode))
}

fn context_with_prompter(
    tree: &Tree,
    mode: PermissionMode,
    prompter: ScriptedPrompter,
) -> ToolContext {
    ToolContext::new(AccessScope::primary_only(tree.root()).expect("a usable primary root"))
        .with_permissions(session(mode).with_prompter(Box::new(prompter)))
}

/// Runs one tool call the way a turn would.
fn call(context: &ToolContext, tool: &str, input: Value) -> ToolResult {
    Registry::builtin()
        .execute(
            &ToolCall {
                id: "call-1".to_string(),
                name: tool.to_string(),
                input,
            },
            context,
        )
        .expect("the tool is advertised")
}

fn succeeds(context: &ToolContext, tool: &str, input: Value) -> String {
    let result = call(context, tool, input);
    assert!(result.ok, "expected success, got {result:?}");
    result.output
}

fn fails(context: &ToolContext, tool: &str, input: Value) -> String {
    let result = call(context, tool, input);
    assert!(!result.ok, "expected a refusal, got {result:?}");
    result.output
}

/// Reads a file through the tool, which is what records the read proof.
fn read_whole(context: &ToolContext, path: &str) -> String {
    succeeds(context, "read_file", json!({ "path": path }))
}

/// The rule/grant key for a command plan: the text *and* the directory.
fn target_of(plan: &CommandPlan) -> String {
    ProposedAction::Command(plan).target()
}

/// A command plan for policy tests: no filesystem effect beyond resolving cwd.
fn plan(tree: &Tree, command: &str) -> CommandPlan {
    CommandPlan::prepare(
        command,
        &AccessScope::primary_only(tree.root()).expect("a usable primary root"),
        None,
        &ToolLimits::default(),
    )
    .expect("a plannable command")
}

// ---------------------------------------------------------------------------
// command classification
// ---------------------------------------------------------------------------

#[test]
fn an_admitted_read_command_plans_an_exact_argv_and_names_no_shell() {
    let CommandEffect::DirectReadOnly { argv } = classify("git status --short") else {
        panic!("`git status --short` must be admitted directly");
    };
    assert_eq!(argv, ["git", "status", "--short"]);
}

#[test]
fn the_admitted_grammar_covers_the_read_and_test_commands_it_claims() {
    for command in [
        "pwd",
        "ls",
        "ls -la src",
        "cat README.md",
        "head -n 20 src/lib.rs",
        "tail -n 5 log.txt",
        "wc -l src/lib.rs",
        "which cargo",
        "echo hello",
        "git status",
        "git diff",
        "git log --oneline",
        "git rev-parse HEAD",
        "cargo --version",
        "cargo -V",
        "cargo metadata --no-deps",
        "cargo metadata --no-deps --offline",
        "cargo fmt --check",
    ] {
        assert!(
            matches!(classify(command), CommandEffect::DirectReadOnly { .. }),
            "`{command}` must be admitted by the direct grammar"
        );
    }
}

#[test]
fn a_destructive_command_is_denied_by_the_effect_it_would_have() {
    let cases = [
        ("rm -rf build", DeniedEffect::FilesystemWrite),
        ("touch new.txt", DeniedEffect::FilesystemWrite),
        ("mv a b", DeniedEffect::FilesystemWrite),
        ("chmod 777 secret", DeniedEffect::FilesystemWrite),
        ("curl https://example.com", DeniedEffect::NetworkAccess),
        ("wget https://example.com", DeniedEffect::NetworkAccess),
        ("ssh host", DeniedEffect::NetworkAccess),
        ("sudo reboot", DeniedEffect::ProcessOrSystem),
        ("bash script.sh", DeniedEffect::ProcessOrSystem),
        ("kill 1", DeniedEffect::ProcessOrSystem),
        ("git push origin main", DeniedEffect::UnsupportedArgument),
        ("git commit -m x", DeniedEffect::UnsupportedArgument),
        ("cargo publish", DeniedEffect::UnsupportedArgument),
        // The ruling: `auto` may write inside the workspace, so it must not
        // also be able to compile and run it.
        ("cargo test", DeniedEffect::ExecutesProjectCode),
        ("cargo build", DeniedEffect::ExecutesProjectCode),
        ("cargo check", DeniedEffect::ExecutesProjectCode),
        ("cargo clippy", DeniedEffect::ExecutesProjectCode),
        ("cargo bench", DeniedEffect::ExecutesProjectCode),
        ("cargo run", DeniedEffect::ExecutesProjectCode),
        // Both of these would do something other than report.
        ("cargo fmt", DeniedEffect::UnsupportedArgument),
        ("cargo metadata", DeniedEffect::UnsupportedArgument),
        ("frobnicate --all", DeniedEffect::UnknownCommand),
    ];
    for (command, expected) in cases {
        assert_eq!(
            classify(command),
            CommandEffect::Denied(expected),
            "`{command}`"
        );
    }
}

#[test]
fn dynamic_shell_syntax_is_never_planned_as_a_direct_argv() {
    for command in [
        "echo $HOME",
        "echo `id`",
        "cat *.rs",
        "cat file?.rs",
        "ls ~",
        "cat a.txt; rm b.txt",
        "cat a.txt | wc -l",
        "cat a.txt && rm b",
        "cat a.txt > out.txt",
        "cat < in.txt",
        "cat a.txt & ",
        "(cat a.txt)",
        "cat $(echo a)",
        "VAR=1 cargo test",
    ] {
        let effect = classify(command);
        assert!(
            matches!(effect, CommandEffect::Denied(_)),
            "`{command}` must not be direct, got {effect:?}"
        );
    }
}

#[test]
fn a_double_dash_stops_flag_parsing_but_not_operand_vetting() {
    // `--` means "no more flags". It has never meant "no more paths", and a
    // parser that stopped checking there would let `cat notes.md -- /etc/passwd`
    // read a file the read tools would refuse.
    for command in [
        "cat notes.md -- /etc/passwd",
        "grep -n needle -- /etc/passwd",
        "ls -- /",
        "cat -- ../outside.txt",
        "wc -l -- /var/log/system.log",
    ] {
        assert_eq!(
            classify(command),
            CommandEffect::Denied(DeniedEffect::UnsupportedArgument),
            "`{command}` escaped through the separator"
        );
    }

    // The separator still works for what it is for: a relative operand that
    // begins with `-` reaches the program instead of the flag grammar.
    assert_eq!(
        classify("grep -n -- -needle src/lib.rs"),
        CommandEffect::DirectReadOnly {
            argv: vec![
                "grep".to_string(),
                "-n".to_string(),
                "--".to_string(),
                "-needle".to_string(),
                "src/lib.rs".to_string(),
            ]
        }
    );
}

#[test]
fn auto_will_not_compile_or_run_the_workspace_it_can_write_to() {
    // The whole reason: `auto` may write inside the workspace without asking.
    // A build script is an ordinary Rust program that `cargo build` executes, so
    // admitting both would be arbitrary code execution with no approval anywhere
    // on the path.
    for command in [
        "cargo test",
        "cargo build",
        "cargo check",
        "cargo clippy",
        "cargo bench",
        "cargo run",
        "cargo install ripgrep",
    ] {
        assert_eq!(
            classify(command),
            CommandEffect::Denied(DeniedEffect::ExecutesProjectCode),
            "`{command}` must not be on the automatic route"
        );
    }
    // What survives only reports.
    for command in [
        "cargo --version",
        "cargo metadata --no-deps",
        "cargo fmt --check",
    ] {
        assert!(
            matches!(classify(command), CommandEffect::DirectReadOnly { .. }),
            "`{command}` should still be admitted"
        );
    }
    // ... and only in the form that reports: a bare `cargo fmt` rewrites the
    // sources, and a bare `cargo metadata` can resolve the graph over the network.
    for command in ["cargo fmt", "cargo metadata"] {
        assert_eq!(
            classify(command),
            CommandEffect::Denied(DeniedEffect::UnsupportedArgument),
            "`{command}`"
        );
    }
    // The refusal has to say what the command would have done, because the
    // model's next move depends on it: "ask the user" and "use another
    // command" are different repairs.
    assert!(
        DeniedEffect::ExecutesProjectCode
            .describe()
            .contains("automatic mode is allowed to write"),
        "{}",
        DeniedEffect::ExecutesProjectCode.describe()
    );
}

#[test]
fn auto_can_write_a_build_script_but_cannot_make_cargo_run_it() {
    // The attack in one test. Writing `build.rs` is a bounded, reversible
    // workspace change, so `auto` allows it. Executing it is not, so `auto`
    // refuses -- and the refusal names the reason rather than the rule.
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Auto);

    succeeds(
        &context,
        "write_file",
        json!({
            "path": "build.rs",
            "content": "fn main() { std::process::Command::new(\"id\").status().unwrap(); }\n",
        }),
    );
    succeeds(
        &context,
        "write_file",
        json!({ "path": ".cargo/config.toml", "content": "[build]\nrustflags = []\n" }),
    );
    assert!(tree.root().join("build.rs").is_file());
    assert!(tree.root().join(".cargo/config.toml").is_file());

    for command in ["cargo test", "cargo build", "cargo check"] {
        let refusal = fails(
            &context,
            "terminal",
            json!({ "action": "exec", "command": command }),
        );
        assert!(refusal.contains("not admitted in auto mode"), "{refusal}");
    }

    // The reporting invocations still work, so `auto` is not merely broken.
    succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cargo --version" }),
    );
}

#[test]
fn a_quoted_metacharacter_is_an_operand_and_not_an_operator() {
    // The lexer has to distinguish `grep ';'` from `grep a; rm b`. If it did
    // not, the safe form would be refused and, worse, the unsafe form could be
    // admitted by quoting it.
    let CommandEffect::DirectReadOnly { argv } = classify("grep -n \"a b\" src/lib.rs") else {
        panic!("a quoted operand must stay one operand");
    };
    assert_eq!(argv, ["grep", "-n", "a b", "src/lib.rs"]);
    assert_eq!(
        classify("grep ';' src/lib.rs"),
        CommandEffect::DirectReadOnly {
            argv: vec![
                "grep".to_string(),
                ";".to_string(),
                "src/lib.rs".to_string()
            ]
        }
    );
}

#[test]
fn an_empty_or_unterminated_command_is_denied_rather_than_guessed() {
    assert_eq!(classify("   "), CommandEffect::Denied(DeniedEffect::Empty));
    assert_eq!(
        classify("grep 'unterminated"),
        CommandEffect::Denied(DeniedEffect::UnsupportedShell)
    );
    assert_eq!(
        classify("cat a\nrm b"),
        CommandEffect::Denied(DeniedEffect::UnsupportedShell)
    );
}

// ---------------------------------------------------------------------------
// policy, evaluated without side effects
// ---------------------------------------------------------------------------

#[test]
fn ask_mode_without_an_approval_channel_fails_closed() {
    let tree = Tree::new();
    let plan = plan(&tree, "git status");
    let decision = session(PermissionMode::Ask).evaluate(ProposedAction::Command(&plan));
    // `ask` asks. With nowhere to ask, the only safe answer is no.
    assert_eq!(decision, PolicyDecision::Prompt);

    let mut noninteractive = session(PermissionMode::Ask);
    let PolicyDecision::Deny { reason, .. } = noninteractive.decide(ProposedAction::Command(&plan))
    else {
        panic!("a noninteractive ask must deny");
    };
    assert!(reason.contains("approval"), "{reason}");
}

#[test]
fn auto_mode_admits_the_direct_grammar_and_denies_everything_else() {
    let tree = Tree::new();
    let admitted = plan(&tree, "cargo --version");
    assert_eq!(
        session(PermissionMode::Auto).evaluate(ProposedAction::Command(&admitted)),
        PolicyDecision::Allow {
            source: AllowSource::AutoMode
        }
    );

    let denied = plan(&tree, "rm -rf src");
    let PolicyDecision::Deny { reason, .. } =
        session(PermissionMode::Auto).evaluate(ProposedAction::Command(&denied))
    else {
        panic!("auto must not admit a destructive command");
    };
    // The refusal names the effect rather than saying "not allowed", so the
    // model can tell "ask the user" from "use a different command".
    assert!(reason.contains("filesystem"), "{reason}");
}

#[test]
fn auto_mode_reviews_an_unknown_command_instead_of_assuming_it_is_safe() {
    let tree = Tree::new();
    let unknown = plan(&tree, "frobnicate --all");
    let PolicyDecision::Deny { reason, .. } =
        session(PermissionMode::Auto).evaluate(ProposedAction::Command(&unknown))
    else {
        panic!("auto must not admit an unrecognized command");
    };
    assert!(reason.contains("not recognized"), "{reason}");
}

#[test]
fn yolo_mode_admits_what_every_other_mode_refuses_and_says_which_source_did_it() {
    let tree = Tree::new();
    let plan = plan(&tree, "rm -rf src");
    assert_eq!(
        session(PermissionMode::Yolo).evaluate(ProposedAction::Command(&plan)),
        PolicyDecision::Allow {
            source: AllowSource::Yolo
        }
    );
    assert!(YOLO_WARNING.contains("yolo"), "{YOLO_WARNING}");
    assert!(
        YOLO_WARNING.contains("no permission check"),
        "{YOLO_WARNING}"
    );
}

#[test]
fn a_configured_deny_rule_outranks_the_mode_that_would_have_allowed_it() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo --version");
    let rules = PermissionRules::new(Vec::new(), vec![Rule::new("terminal", target_of(&plan))]);
    let denied = session(PermissionMode::Auto).with_rules(rules);
    let PolicyDecision::Deny { reason, .. } = denied.evaluate(ProposedAction::Command(&plan))
    else {
        panic!("a deny rule must beat auto admission");
    };
    assert!(reason.contains("rule"), "{reason}");
}

#[test]
fn a_configured_allow_rule_admits_without_an_approval() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo publish");
    let rules = PermissionRules::new(vec![Rule::new("terminal", target_of(&plan))], Vec::new());
    let allowed = session(PermissionMode::Ask).with_rules(rules);
    assert_eq!(
        allowed.evaluate(ProposedAction::Command(&plan)),
        PolicyDecision::Allow {
            source: AllowSource::ConfiguredRule
        }
    );
}

#[test]
fn a_session_grant_is_exact_and_does_not_spread_to_a_neighbour() {
    let tree = Tree::new();
    let granted = plan(&tree, "cargo --version");
    let neighbour = plan(&tree, "cargo --version --list");
    let mut with_grant = session(PermissionMode::Ask);
    with_grant.grant(Grant::new("terminal", target_of(&granted)));

    assert_eq!(
        with_grant.evaluate(ProposedAction::Command(&granted)),
        PolicyDecision::Allow {
            source: AllowSource::SessionGrant
        }
    );
    assert_eq!(
        with_grant.evaluate(ProposedAction::Command(&neighbour)),
        PolicyDecision::Prompt,
        "a grant for one command must not cover a longer one"
    );
}

// ---------------------------------------------------------------------------
// interactive approval
// ---------------------------------------------------------------------------

#[test]
fn a_grant_for_a_command_does_not_follow_it_into_another_directory() {
    // The same words in a different directory are a different command: `cat
    // notes.md` in the workspace and in a directory `--add-dir` opened read two
    // different files. A key that was only the command text would let one
    // approval buy both.
    let tree = Tree::new();
    let shared = TempDir::new().expect("shared");
    let shared_root = shared.path().canonicalize().expect("canonicalize shared");
    tree.mkdir("inner");
    let scope = AccessScope::new(tree.root(), [&shared_root]).expect("usable roots");
    let limits = ToolLimits::default();

    let here = CommandPlan::prepare("pwd", &scope, None, &limits).expect("plan");
    let inner = CommandPlan::prepare("pwd", &scope, Some("inner"), &limits).expect("plan");
    let added = CommandPlan::prepare("pwd", &scope, Some(shared_root.to_str().unwrap()), &limits)
        .expect("plan");

    // Three different keys for three different questions.
    let keys = [target_of(&here), target_of(&inner), target_of(&added)];
    let mut unique = keys.clone().to_vec();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3, "{keys:?}");

    let mut asking = session(PermissionMode::Ask);
    asking.grant(Grant::new("terminal", target_of(&here)));
    assert_eq!(
        asking.evaluate(ProposedAction::Command(&here)),
        PolicyDecision::Allow {
            source: AllowSource::SessionGrant
        }
    );
    for elsewhere in [&inner, &added] {
        assert_eq!(
            asking.evaluate(ProposedAction::Command(elsewhere)),
            PolicyDecision::Prompt,
            "a grant in one directory covered another: {}",
            target_of(elsewhere)
        );
    }

    // A configured rule is keyed the same way, so writing one for the workspace
    // does not silently authorize an added root.
    let ruled = session(PermissionMode::Ask).with_rules(PermissionRules::new(
        vec![Rule::new("terminal", target_of(&here))],
        Vec::new(),
    ));
    assert_eq!(
        ruled.evaluate(ProposedAction::Command(&added)),
        PolicyDecision::Prompt
    );
}

#[test]
fn an_approval_answered_once_covers_this_call_and_not_the_next() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo publish");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Once]);
    let mut asking = session(PermissionMode::Ask).with_prompter(Box::new(prompter.clone()));

    assert_eq!(
        asking.decide(ProposedAction::Command(&plan)),
        PolicyDecision::Allow {
            source: AllowSource::InteractiveOnce
        }
    );
    assert!(asking.grants().is_empty(), "`once` must not persist");
    assert_eq!(prompter.asked(), [format!("terminal:{}", target_of(&plan))]);
}

#[test]
fn an_approval_answered_always_records_one_exact_session_grant() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo publish");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Always]);
    let mut asking = session(PermissionMode::Ask).with_prompter(Box::new(prompter.clone()));

    assert_eq!(
        asking.decide(ProposedAction::Command(&plan)),
        PolicyDecision::Allow {
            source: AllowSource::InteractiveAlways
        }
    );
    assert_eq!(asking.grants(), [Grant::new("terminal", target_of(&plan))]);

    // The second call is answered from the grant, so the prompter is not asked
    // again.
    assert_eq!(
        asking.decide(ProposedAction::Command(&plan)),
        PolicyDecision::Allow {
            source: AllowSource::SessionGrant
        }
    );
    assert_eq!(prompter.asked().len(), 1);
}

#[test]
fn an_edit_prompt_shows_what_is_being_replaced_and_with_what() {
    let tree = Tree::new();
    tree.write("notes.md", "alpha beta gamma\n");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Deny]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());
    read_whole(&context, "notes.md");

    fails(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "beta", "new_string": "DELTA" }),
    );

    let request = prompter.last().expect("the edit asked a question");
    assert_eq!(request.tool, "edit_file");
    assert_eq!(request.target, "notes.md");
    // The bytes on both sides, the current size, and both digests. "Edit
    // notes.md" would not have been a question anyone could answer.
    assert!(
        request.summary.contains("replace \"beta\" with \"DELTA\""),
        "{}",
        request.summary
    );
    assert!(request.summary.contains("17 bytes"), "{}", request.summary);
    assert!(request.summary.contains("sha256"), "{}", request.summary);
    // And "always" says it is broader than the change being shown.
    assert!(
        request
            .always_scope
            .contains("every future edit_file to `notes.md`"),
        "{}",
        request.always_scope
    );
    assert!(
        request.always_scope.contains("whatever its contents"),
        "{}",
        request.always_scope
    );
    assert_eq!(tree.read("notes.md"), "alpha beta gamma\n");
}

#[test]
fn a_write_prompt_shows_a_bounded_preview_with_its_size_and_digest() {
    let tree = Tree::new();
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Deny]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());

    fails(
        &context,
        "write_file",
        json!({ "path": "new.txt", "content": "hello world\n" }),
    );
    let request = prompter.last().expect("the write asked a question");
    assert!(
        request.summary.contains("write 12 bytes"),
        "{}",
        request.summary
    );
    assert!(
        request.summary.contains("does not exist yet"),
        "{}",
        request.summary
    );
    // Escaped and on one line: a payload must not be able to reflow the prompt
    // it is being shown in.
    assert!(
        request.summary.contains("hello world\\n"),
        "{}",
        request.summary
    );
    assert!(!request.summary.contains('\n'), "{}", request.summary);
}

#[test]
fn a_prompt_bounds_the_excerpt_it_shows() {
    let tree = Tree::new();
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Deny]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());

    fails(
        &context,
        "write_file",
        json!({ "path": "big.txt", "content": "A".repeat(4000) }),
    );
    let request = prompter.last().expect("the write asked a question");
    assert!(
        request.summary.contains("write 4000 bytes"),
        "{}",
        request.summary
    );
    // Bounded, and the clipping is visible rather than silent.
    assert!(request.summary.contains('\u{2026}'), "{}", request.summary);
    assert!(
        request.summary.len() < 400,
        "the prompt was {} bytes",
        request.summary.len()
    );
}

#[test]
fn a_command_prompt_names_the_directory_the_command_would_run_in() {
    let tree = Tree::new();
    tree.mkdir("inner");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Deny]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());

    fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cargo test", "cwd": "inner" }),
    );
    let request = prompter.last().expect("the command asked a question");
    assert!(
        request.summary.contains("cargo test"),
        "{}",
        request.summary
    );
    assert!(request.summary.contains("inner"), "{}", request.summary);
    assert!(request.target.contains("inner"), "{}", request.target);
    assert!(
        request.always_scope.contains("every future run of exactly"),
        "{}",
        request.always_scope
    );
}

#[test]
fn a_denied_approval_refuses_and_records_nothing() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo publish");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Deny]);
    let mut asking = session(PermissionMode::Ask).with_prompter(Box::new(prompter));

    let PolicyDecision::Deny { reason, .. } = asking.decide(ProposedAction::Command(&plan)) else {
        panic!("a denied approval must deny");
    };
    assert!(reason.contains("declined"), "{reason}");
    assert!(asking.grants().is_empty());
}

#[test]
fn a_broken_approval_channel_denies_rather_than_defaulting_to_yes() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo publish");
    let mut asking =
        session(PermissionMode::Ask).with_prompter(Box::new(ScriptedPrompter::broken()));

    let PolicyDecision::Deny { reason, .. } = asking.decide(ProposedAction::Command(&plan)) else {
        panic!("a broken channel must deny");
    };
    assert!(reason.contains("approval channel"), "{reason}");
}

// ---------------------------------------------------------------------------
// authority lifecycle
// ---------------------------------------------------------------------------

#[test]
fn an_authority_executes_once_and_a_replay_is_refused() {
    let tree = Tree::new();
    let plan = plan(&tree, "cargo --version");
    let mut owner = session(PermissionMode::Auto);
    let authority = owner.mint_command(plan, AllowSource::AutoMode);
    // The authority records which decision produced it, so an audit can say
    // *why* something was allowed and not only that it was.
    assert_eq!(authority.source(), AllowSource::AutoMode);

    owner.consume(&authority).expect("the first use is allowed");
    assert_eq!(
        owner.consume(&authority),
        Err(AuthorityError::Consumed),
        "a cloned authority is still one use"
    );
}

#[test]
fn an_authority_minted_by_one_session_is_unknown_to_another() {
    let tree = Tree::new();
    let mut minter = session(PermissionMode::Auto);
    let authority = minter.mint_command(plan(&tree, "cargo --version"), AllowSource::AutoMode);

    let mut stranger = session(PermissionMode::Auto);
    assert_eq!(stranger.consume(&authority), Err(AuthorityError::Unknown));
}

#[test]
fn two_authorities_never_share_a_nonce() {
    let tree = Tree::new();
    let mut owner = session(PermissionMode::Auto);
    let first = owner.mint_command(plan(&tree, "cargo --version"), AllowSource::AutoMode);
    let second = owner.mint_command(plan(&tree, "cargo --version"), AllowSource::AutoMode);
    assert_ne!(first.nonce(), second.nonce());
    owner.consume(&first).expect("the first is usable");
    owner.consume(&second).expect("the second is independent");
}

#[test]
fn a_revalidation_failure_still_burns_the_authority_it_was_given() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let root = tree.root().to_path_buf();
    let context = context(&tree, PermissionMode::Auto).with_race_interlude(Arc::new(move || {
        fs::write(root.join("notes.md"), "swapped\n").expect("swap the preimage");
    }));
    read_whole(&context, "notes.md");

    assert_eq!(context.permissions().ledger_counts(), (0, 0));
    let result = call(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "original", "new_string": "edited" }),
    );
    assert!(!result.ok, "{result:?}");
    // One authority was issued and one was spent, even though nothing was
    // written: a retry has to be authorized again rather than reusing an answer
    // about a world that has moved.
    assert_eq!(context.permissions().ledger_counts(), (1, 1));
}

#[test]
fn a_command_fingerprint_covers_the_command_the_cwd_and_the_environment() {
    let tree = Tree::new();
    tree.mkdir("inner");
    let scope = AccessScope::primary_only(tree.root()).expect("root");
    let limits = ToolLimits::default();

    let base = CommandPlan::prepare("cargo --version", &scope, None, &limits).expect("plan");
    let other_command = CommandPlan::prepare("cargo -V", &scope, None, &limits).expect("plan");
    let other_cwd =
        CommandPlan::prepare("cargo --version", &scope, Some("inner"), &limits).expect("plan");

    assert_ne!(base.fingerprint(), other_command.fingerprint());
    assert_ne!(base.fingerprint(), other_cwd.fingerprint());
    // Same inputs, same fingerprint: the check is a function of the plan and not
    // of when it ran.
    assert_eq!(
        base.fingerprint(),
        CommandPlan::prepare("cargo --version", &scope, None, &limits)
            .expect("plan")
            .fingerprint()
    );

    // The environment is a fixed, named set. Nothing is inherited, so a bearer
    // token in the parent process cannot reach a child.
    let names: Vec<&str> = base
        .environment()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["HOME", "LANG", "LC_ALL", "PATH"]);
}

#[test]
fn an_admitted_command_names_its_route_and_a_reviewed_one_names_the_shell() {
    let tree = Tree::new();
    assert!(matches!(
        plan(&tree, "cargo --version").route(),
        CommandRoute::Direct { .. }
    ));
    assert!(matches!(
        plan(&tree, "cargo --version && rm -rf /").route(),
        CommandRoute::Shell { .. }
    ));
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

#[test]
fn write_file_creates_a_new_file_and_reports_what_it_wrote() {
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(
        &context,
        "write_file",
        json!({ "path": "notes/new.md", "content": "hello\n" }),
    );
    assert!(output.contains("notes/new.md"), "{output}");
    assert_eq!(tree.read("notes/new.md"), "hello\n");
}

#[test]
fn write_file_refuses_to_overwrite_a_file_the_model_never_read() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let context = context(&tree, PermissionMode::Auto);
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "replaced\n" }),
    );
    assert!(refusal.contains("read"), "{refusal}");
    assert_eq!(tree.read("notes.md"), "original\n", "the file was replaced");
}

#[test]
fn a_refused_write_creates_none_of_the_directories_it_would_have_needed() {
    // Preparation resolves the path but must create nothing: if it created the
    // parents first, a denied decision would still have changed the workspace.
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Ask);
    fails(
        &context,
        "write_file",
        json!({ "path": "a/b/c.txt", "content": "hello\n" }),
    );
    assert!(
        tree.entries(".").is_empty(),
        "preparation created a directory"
    );
}

#[test]
fn an_approved_edit_runs_in_ask_mode_and_asks_exactly_once() {
    let tree = Tree::new();
    tree.write("notes.md", "alpha\n");
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Once]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());

    read_whole(&context, "notes.md");
    succeeds(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "alpha", "new_string": "beta" }),
    );
    assert_eq!(tree.read("notes.md"), "beta\n");
    // The question named the tool and the exact target, which is also what an
    // "always" answer would have recorded.
    assert_eq!(prompter.asked(), ["edit_file:notes.md"]);
}

#[test]
fn a_partial_read_is_not_a_read_proof() {
    let tree = Tree::new();
    tree.write("notes.md", "one\ntwo\nthree\n");
    let context = context(&tree, PermissionMode::Auto);
    // The model saw one line of three. That is not "I know what is in this
    // file", so it cannot authorize replacing the other two.
    succeeds(
        &context,
        "read_file",
        json!({ "path": "notes.md", "line_count": 1 }),
    );
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "replaced\n" }),
    );
    assert!(refusal.contains("whole"), "{refusal}");
    assert_eq!(tree.read("notes.md"), "one\ntwo\nthree\n");
}

#[test]
fn a_complete_read_authorizes_the_overwrite() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    succeeds(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "replaced\n" }),
    );
    assert_eq!(tree.read("notes.md"), "replaced\n");
}

#[test]
fn a_preimage_that_changed_between_the_read_and_the_write_is_refused() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    // Someone else edited the file. The model's proof describes a file that no
    // longer exists, so acting on it would silently discard their work.
    tree.write("notes.md", "someone else was here\n");

    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "replaced\n" }),
    );
    assert!(refusal.contains("changed"), "{refusal}");
    assert_eq!(tree.read("notes.md"), "someone else was here\n");
}

#[test]
fn a_write_larger_than_the_bound_is_refused_before_anything_is_staged() {
    let tree = Tree::new();
    let context = context_with_limits(
        &tree,
        PermissionMode::Auto,
        ToolLimits {
            max_mutation_bytes: 8,
            ..ToolLimits::default()
        },
    );
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "big.txt", "content": "0123456789" }),
    );
    assert!(refusal.contains("bytes"), "{refusal}");
    assert!(
        tree.entries(".").is_empty(),
        "a staging file was left behind"
    );
}

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

#[test]
fn edit_file_replaces_the_one_occurrence_it_was_given() {
    let tree = Tree::new();
    tree.write("notes.md", "alpha\nbeta\ngamma\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    let output = succeeds(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "beta", "new_string": "delta" }),
    );
    assert!(output.contains("notes.md"), "{output}");
    assert_eq!(tree.read("notes.md"), "alpha\ndelta\ngamma\n");
}

#[test]
fn edit_file_refuses_a_repeated_old_string_rather_than_guessing_which_one() {
    let tree = Tree::new();
    tree.write("notes.md", "beta\nbeta\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    let refusal = fails(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "beta", "new_string": "delta" }),
    );
    assert!(refusal.contains("2 times"), "{refusal}");
    assert_eq!(tree.read("notes.md"), "beta\nbeta\n");
}

#[test]
fn edit_file_refuses_an_old_string_that_is_not_there() {
    let tree = Tree::new();
    tree.write("notes.md", "alpha\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    let refusal = fails(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "beta", "new_string": "delta" }),
    );
    assert!(refusal.contains("not found"), "{refusal}");
    assert_eq!(tree.read("notes.md"), "alpha\n");
}

#[test]
fn an_edit_that_changes_nothing_says_so_and_does_not_touch_the_file() {
    let tree = Tree::new();
    let path = tree.write("notes.md", "alpha\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    let before = fs::metadata(&path)
        .expect("stat")
        .modified()
        .expect("mtime");

    let output = succeeds(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "alpha", "new_string": "alpha" }),
    );
    assert!(output.starts_with("No changes to "), "{output}");
    assert_eq!(tree.read("notes.md"), "alpha\n");
    assert_eq!(
        fs::metadata(&path)
            .expect("stat")
            .modified()
            .expect("mtime"),
        before,
        "a no-op must not rewrite the file"
    );
}

#[test]
fn edit_file_refuses_a_file_that_does_not_exist() {
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Auto);
    let refusal = fails(
        &context,
        "edit_file",
        json!({ "path": "missing.md", "old_string": "a", "new_string": "b" }),
    );
    assert!(refusal.contains("no such path"), "{refusal}");
}

// ---------------------------------------------------------------------------
// namespace safety
// ---------------------------------------------------------------------------

#[test]
fn a_target_that_is_a_symlink_is_refused_rather_than_followed() {
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    let victim = outside.path().join("victim.txt");
    fs::write(&victim, "outside content\n").expect("write victim");
    symlink(&victim, tree.root().join("link.md")).expect("create the symlink");

    let context = context(&tree, PermissionMode::Yolo);
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "link.md", "content": "captured\n" }),
    );
    assert!(refusal.contains("symbolic link"), "{refusal}");
    assert_eq!(
        fs::read_to_string(&victim).expect("read victim"),
        "outside content\n",
        "the symlink target was written through"
    );
}

#[test]
fn a_parent_component_that_is_a_symlink_is_refused() {
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    symlink(outside.path(), tree.root().join("escape")).expect("create the directory symlink");

    let context = context(&tree, PermissionMode::Yolo);
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": "escape/captured.txt", "content": "captured\n" }),
    );
    assert!(refusal.contains("symbolic link"), "{refusal}");
    assert!(
        !outside.path().join("captured.txt").exists(),
        "the write escaped through a directory symlink"
    );
}

#[test]
fn a_target_outside_every_authorized_root_is_refused() {
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    let target = outside.path().join("captured.txt");
    let context = context(&tree, PermissionMode::Yolo);

    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": target.to_str().unwrap(), "content": "captured\n" }),
    );
    assert!(refusal.contains("outside"), "{refusal}");
    assert!(!target.exists());
}

#[test]
fn auto_mode_will_not_write_into_an_additional_root() {
    let tree = Tree::new();
    let shared = TempDir::new().expect("shared");
    let shared_root = shared.path().canonicalize().expect("canonicalize shared");
    let context =
        ToolContext::new(AccessScope::new(tree.root(), [&shared_root]).expect("usable roots"))
            .with_permissions(session(PermissionMode::Auto));

    let target = shared_root.join("new.txt");
    let refusal = fails(
        &context,
        "write_file",
        json!({ "path": target.to_str().unwrap(), "content": "hello\n" }),
    );
    // Reading an added directory is what `--add-dir` grants. Writing into it is
    // a separate decision, and auto does not make it.
    assert!(refusal.contains("workspace"), "{refusal}");
    assert!(!target.exists());
}

#[test]
fn a_preimage_replaced_after_admission_is_refused_and_the_file_is_left_alone() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let root = tree.root().to_path_buf();
    let context = context(&tree, PermissionMode::Auto).with_race_interlude(Arc::new(move || {
        // The moment between "authorized" and "committed" is the race window.
        fs::write(root.join("notes.md"), "swapped\n").expect("swap the preimage");
    }));

    read_whole(&context, "notes.md");
    let result = call(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "original", "new_string": "edited" }),
    );
    assert!(!result.ok, "{result:?}");
    assert!(
        result.fatal,
        "a lost authority must not be a retryable result"
    );
    assert!(result.output.contains("changed"), "{}", result.output);
    assert_eq!(tree.read("notes.md"), "swapped\n");
    assert_eq!(tree.entries("."), ["notes.md"], "a staging file survived");
}

#[test]
fn a_target_replaced_by_a_symlink_after_admission_is_refused() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let outside = TempDir::new().expect("outside");
    let victim = outside.path().join("victim.txt");
    fs::write(&victim, "outside content\n").expect("write victim");

    let root = tree.root().to_path_buf();
    let victim_path = victim.clone();
    let context = context(&tree, PermissionMode::Auto).with_race_interlude(Arc::new(move || {
        fs::remove_file(root.join("notes.md")).expect("remove the target");
        symlink(&victim_path, root.join("notes.md")).expect("plant the symlink");
    }));

    read_whole(&context, "notes.md");
    let result = call(
        &context,
        "edit_file",
        json!({ "path": "notes.md", "old_string": "original", "new_string": "edited" }),
    );
    assert!(!result.ok, "{result:?}");
    assert_eq!(
        fs::read_to_string(&victim).expect("read victim"),
        "outside content\n"
    );
}

#[test]
fn a_replacement_preserves_the_file_mode() {
    let tree = Tree::new();
    let path = tree.write("script.sh", "old\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).expect("set mode");

    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "script.sh");
    succeeds(
        &context,
        "write_file",
        json!({ "path": "script.sh", "content": "new\n" }),
    );
    let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o750, "the replacement changed the mode to {mode:o}");
}

#[test]
fn a_replacement_leaves_no_staging_file_behind() {
    let tree = Tree::new();
    tree.write("notes.md", "old\n");
    let context = context(&tree, PermissionMode::Auto);
    read_whole(&context, "notes.md");
    succeeds(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "new\n" }),
    );
    assert_eq!(tree.entries("."), ["notes.md"]);
}

#[test]
fn a_refused_mutation_leaves_no_staging_file_behind() {
    let tree = Tree::new();
    tree.write("notes.md", "old\n");
    let context = context(&tree, PermissionMode::Ask);
    read_whole(&context, "notes.md");
    fails(
        &context,
        "write_file",
        json!({ "path": "notes.md", "content": "new\n" }),
    );
    assert_eq!(tree.entries("."), ["notes.md"]);
    assert_eq!(tree.read("notes.md"), "old\n");
}

// ---------------------------------------------------------------------------
// create_folder
// ---------------------------------------------------------------------------

#[test]
fn create_folder_creates_the_whole_path_it_was_given() {
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(&context, "create_folder", json!({ "path": "a/b/c" }));
    assert!(output.contains("a/b/c"), "{output}");
    assert!(tree.root().join("a/b/c").is_dir());
}

#[test]
fn create_folder_says_when_the_directory_was_already_there() {
    let tree = Tree::new();
    tree.mkdir("a/b");
    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(&context, "create_folder", json!({ "path": "a/b" }));
    assert!(output.contains("already"), "{output}");
}

#[test]
fn create_folder_refuses_a_path_outside_the_workspace() {
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    let target = outside.path().join("made");
    let context = context(&tree, PermissionMode::Yolo);
    fails(
        &context,
        "create_folder",
        json!({ "path": target.to_str().unwrap() }),
    );
    assert!(!target.exists());
}

#[test]
fn create_folder_refuses_a_path_whose_parent_is_a_file() {
    let tree = Tree::new();
    tree.write("file.txt", "x");
    let context = context(&tree, PermissionMode::Auto);
    let refusal = fails(
        &context,
        "create_folder",
        json!({ "path": "file.txt/inner" }),
    );
    assert!(refusal.contains("not a directory"), "{refusal}");
}

// ---------------------------------------------------------------------------
// terminal
// ---------------------------------------------------------------------------

#[test]
fn terminal_advertises_exec_and_no_durable_action() {
    let spec = Registry::builtin()
        .spec("terminal")
        .expect("terminal is advertised");
    let schema = spec.advertisement();
    assert_eq!(
        schema["inputSchema"]["properties"]["action"]["enum"],
        json!(["exec"])
    );
    let rendered = schema.to_string();
    for durable in [
        "start",
        "session_id",
        "monitor",
        "resize",
        "signal",
        "close",
        "wait",
        "screen",
        "lease",
        "background",
    ] {
        assert!(
            !rendered.contains(durable),
            "the terminal schema mentions the durable surface `{durable}`: {rendered}"
        );
    }
}

#[test]
fn terminal_runs_an_admitted_command_and_reports_its_exit_status() {
    let tree = Tree::new();
    tree.write("notes.md", "alpha\nbeta\n");
    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "wc -l notes.md" }),
    );
    assert!(output.contains("<exit_code>0</exit_code>"), "{output}");
    assert!(output.contains('2'), "{output}");
}

#[test]
fn terminal_reports_a_nonzero_exit_status_as_a_fact_rather_than_a_failure() {
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Auto);
    let result = call(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cat missing.txt" }),
    );
    // The command ran. It said no. That is an answer the model can use, not a
    // reason to end the turn.
    assert!(result.ok, "{result:?}");
    assert!(
        !result.output.contains("<exit_code>0</exit_code>"),
        "{}",
        result.output
    );
}

#[test]
fn terminal_bounds_the_output_it_returns_and_says_that_it_did() {
    let tree = Tree::new();
    tree.write("big.txt", &"x".repeat(4096));
    let context = context_with_limits(
        &tree,
        PermissionMode::Auto,
        ToolLimits {
            max_command_output_bytes: 64,
            ..ToolLimits::default()
        },
    );
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cat big.txt" }),
    );
    assert!(output.contains("truncated"), "{output}");
    assert!(output.len() < 1024, "output was {} bytes", output.len());
}

#[test]
fn terminal_kills_a_command_that_outruns_its_timeout() {
    let tree = Tree::new();
    let context = context_with_limits(
        &tree,
        PermissionMode::Yolo,
        ToolLimits {
            command_timeout_ms: 200,
            ..ToolLimits::default()
        },
    );
    let started = Instant::now();
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "sleep 30" }),
    );
    assert!(refusal.contains("timed out"), "{refusal}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timeout did not fire: {:?}",
        started.elapsed()
    );
}

#[test]
fn terminal_stops_when_the_turn_is_cancelled() {
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let context = ToolContext::new(AccessScope::primary_only(tree.root()).expect("root"))
        .with_permissions(session(PermissionMode::Yolo))
        .with_cancel(cancel.clone());

    let watcher = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        watcher.cancel();
    });

    let started = Instant::now();
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "sleep 30" }),
    );
    assert!(refusal.contains("cancelled"), "{refusal}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation was ignored for {:?}",
        started.elapsed()
    );
}

#[test]
fn a_direct_operand_that_resolves_outside_the_workspace_is_refused() {
    // The text classifier cannot see this: `escape.txt` is a perfectly ordinary
    // relative name. Only resolving it shows that `cat escape.txt` would read a
    // file the read tools refuse.
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, "SENSITIVE-OUTSIDE-VALUE\n").expect("write the outside file");
    symlink(&secret, tree.root().join("escape.txt")).expect("plant the symlink");

    let context = context(&tree, PermissionMode::Auto);
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cat escape.txt" }),
    );
    assert!(refusal.contains("outside the authorized"), "{refusal}");
    assert!(
        !refusal.contains("SENSITIVE-OUTSIDE-VALUE"),
        "the refusal leaked the file it refused"
    );
}

#[test]
fn a_direct_operand_that_is_a_symlink_inside_the_workspace_is_allowed() {
    // The counterpart. Refusing every symlink would make the grammar useless in
    // a real repository, and an in-workspace link resolves to a file the read
    // tools would have handed over anyway.
    let tree = Tree::new();
    tree.write("real.txt", "inside content\n");
    symlink(tree.root().join("real.txt"), tree.root().join("link.txt")).expect("plant the symlink");

    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "cat link.txt" }),
    );
    assert!(output.contains("inside content"), "{output}");
}

#[test]
fn an_operand_that_names_nothing_is_not_treated_as_an_escape() {
    // A grep pattern, a git revision, and a filename that does not exist yet all
    // look identical to a path. Refusing them would leave the grammar able to
    // express almost nothing.
    let tree = Tree::new();
    tree.write("notes.md", "needle here\n");
    let context = context(&tree, PermissionMode::Auto);
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "grep -n needle notes.md" }),
    );
    assert!(output.contains("needle"), "{output}");
}

#[test]
fn a_command_that_leaves_a_process_holding_the_pipe_still_returns() {
    // `sh -c 'sleep 30 & echo started'` exits at once, but the background sleep
    // inherited stdout and keeps the pipe open. Joining the reader thread here
    // would block the whole turn for thirty seconds.
    let tree = Tree::new();
    let context = context(&tree, PermissionMode::Yolo);
    let started = Instant::now();
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "sleep 30 & echo started" }),
    );
    assert!(output.contains("started"), "{output}");
    assert!(output.contains("<exit_code>0</exit_code>"), "{output}");
    // The wait is bounded and the shortfall is disclosed.
    assert!(output.contains("stream still open"), "{output}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the drain blocked for {:?}",
        started.elapsed()
    );
}

#[test]
fn a_timeout_kills_the_whole_process_group_and_not_only_the_child() {
    // The shell forks, so killing the process fxr spawned leaves the two sleeps
    // alive and holding the pipe. Only a group kill ends them.
    let tree = Tree::new();
    let context = context_with_limits(
        &tree,
        PermissionMode::Yolo,
        ToolLimits {
            command_timeout_ms: 200,
            ..ToolLimits::default()
        },
    );
    let started = Instant::now();
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "sleep 30 & sleep 30" }),
    );
    assert!(refusal.contains("timed out"), "{refusal}");
    // The discriminator. If only the forked shell had been killed, the two
    // sleeps would still hold the pipe and the drain would report itself
    // stalled. A closed pipe is the evidence that the group is gone.
    assert!(
        !refusal.contains("stream still open"),
        "a process outlived the group kill: {refusal}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timeout took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_cancellation_kills_the_whole_process_group_and_not_only_the_child() {
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let context = ToolContext::new(AccessScope::primary_only(tree.root()).expect("root"))
        .with_permissions(session(PermissionMode::Yolo))
        .with_cancel(cancel.clone());

    let watcher = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        watcher.cancel();
    });

    let started = Instant::now();
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "sleep 30 & sleep 30" }),
    );
    assert!(refusal.contains("cancelled"), "{refusal}");
    assert!(
        !refusal.contains("stream still open"),
        "a process outlived the group kill: {refusal}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation took {:?}",
        started.elapsed()
    );
}

#[test]
fn terminal_refuses_a_working_directory_outside_the_workspace() {
    let tree = Tree::new();
    let outside = TempDir::new().expect("outside");
    let context = context(&tree, PermissionMode::Yolo);
    let refusal = fails(
        &context,
        "terminal",
        json!({
            "action": "exec",
            "command": "pwd",
            "cwd": outside.path().to_str().unwrap(),
        }),
    );
    assert!(refusal.contains("outside"), "{refusal}");
}

#[test]
fn terminal_denies_a_destructive_command_in_auto_mode_and_runs_nothing() {
    let tree = Tree::new();
    tree.write("victim.txt", "still here\n");
    let context = context(&tree, PermissionMode::Auto);
    let refusal = fails(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "rm victim.txt" }),
    );
    assert!(refusal.contains("filesystem"), "{refusal}");
    assert_eq!(tree.read("victim.txt"), "still here\n");
}

#[test]
fn an_approved_command_runs_through_the_platform_shell_with_a_clean_environment() {
    let tree = Tree::new();
    let prompter = ScriptedPrompter::new(vec![ApprovalAnswer::Once]);
    let context = context_with_prompter(&tree, PermissionMode::Ask, prompter.clone());

    // Dynamic shell syntax can only run on the reviewed route. `CARGO_PKG_NAME`
    // is set in this test process, so an inherited environment would print
    // `[fxr]` and a built one prints `[]`.
    assert_eq!(std::env::var("CARGO_PKG_NAME").as_deref(), Ok("fxr"));
    let output = succeeds(
        &context,
        "terminal",
        json!({ "action": "exec", "command": "echo \"[$CARGO_PKG_NAME]\"" }),
    );
    assert!(
        output.contains("[]") && !output.contains("[fxr]"),
        "the child inherited the environment: {output}"
    );
    assert_eq!(prompter.asked().len(), 1);
}

// ---------------------------------------------------------------------------
// registry and advertisement
// ---------------------------------------------------------------------------

#[test]
fn the_registry_advertises_the_read_tools_then_the_mutating_tools() {
    assert_eq!(
        ADVERTISED_TOOLS,
        [
            "list_files",
            "glob_files",
            "grep_files",
            "read_file",
            "write_file",
            "edit_file",
            "create_folder",
            "terminal",
        ]
    );
    assert_eq!(Registry::builtin().names(), ADVERTISED_TOOLS);
}

#[test]
fn the_advertisement_still_names_no_deferred_surface() {
    let rendered = serde_json::to_string(&Registry::builtin().advertisement()).expect("json");
    for deferred in [
        "delete_file",
        "rename_file",
        "copy_file",
        "file_info",
        "memory",
        "semantic_search",
        "open_file",
        "web_fetch",
        "web_search",
        "todo_write",
    ] {
        assert!(
            !rendered.contains(deferred),
            "the advertisement names `{deferred}`"
        );
    }
}

#[test]
fn every_advertised_schema_is_closed_and_declares_only_declared_requirements() {
    for spec in Registry::builtin().specs() {
        let schema = spec.advertisement();
        assert_eq!(
            schema["inputSchema"]["additionalProperties"],
            json!(false),
            "{} has an open schema",
            spec.name()
        );
        let properties = schema["inputSchema"]["properties"]
            .as_object()
            .expect("an object");
        if let Some(required) = schema["inputSchema"]["required"].as_array() {
            for name in required {
                let name = name.as_str().expect("a name");
                assert!(
                    properties.contains_key(name),
                    "{} requires the undeclared `{name}`",
                    spec.name()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the turn: a lost authority ends it
// ---------------------------------------------------------------------------

/// A provider that replays scripted completions in order.
struct ScriptedProvider {
    steps: Mutex<VecDeque<Completion>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedProvider {
    fn new(steps: Vec<Completion>) -> Self {
        Self {
            steps: Mutex::new(steps.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().expect("requests").clone()
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for ScriptedProvider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        let completion = self
            .steps
            .lock()
            .expect("steps")
            .pop_front()
            .expect("the script has a step for this request");
        if !completion.text.is_empty() {
            deltas.text_delta(&completion.text).expect("delta");
        }
        Ok(completion)
    }
}

fn calls_step(calls: Vec<ToolCall>) -> Completion {
    Completion {
        text: String::new(),
        tool_calls: calls,
        finish_reason: FinishReason::ToolCalls,
        usage: Usage::default(),
        provider_detail: None,
    }
}

fn answer_step(text: &str) -> Completion {
    Completion {
        text: text.to_string(),
        tool_calls: Vec::new(),
        finish_reason: FinishReason::Stop,
        usage: Usage::default(),
        provider_detail: None,
    }
}

fn turn(prompt: &str, tools: ToolContext) -> TurnRequest {
    TurnRequest {
        model: "vendor/model".to_string(),
        prompt: prompt.to_string(),
        history: Vec::new(),
        max_steps: 6,
        max_attempts: 1,
        cancel: CancelToken::new(),
        tools,
    }
}

fn kinds(sink: &RecordingSink) -> Vec<&'static str> {
    sink.events()
        .iter()
        .map(|event| match event {
            Event::AssistantDelta { .. } => "assistant_delta",
            Event::ToolStart { .. } => "tool_start",
            Event::ToolResult { .. } => "tool_result",
            Event::Final { .. } => "final",
            Event::Error { .. } => "error",
        })
        .collect()
}

#[tokio::test]
async fn a_mutation_that_loses_its_authority_ends_the_turn_without_continuing() {
    let tree = Tree::new();
    tree.write("notes.md", "original\n");
    let root = tree.root().to_path_buf();
    let context = context(&tree, PermissionMode::Auto).with_race_interlude(Arc::new(move || {
        fs::write(root.join("notes.md"), "swapped\n").expect("swap the preimage");
    }));

    let provider = ScriptedProvider::new(vec![
        calls_step(vec![ToolCall {
            id: "c1".to_string(),
            name: "read_file".to_string(),
            input: json!({ "path": "notes.md" }),
        }]),
        calls_step(vec![ToolCall {
            id: "c2".to_string(),
            name: "edit_file".to_string(),
            input: json!({
                "path": "notes.md",
                "old_string": "original",
                "new_string": "edited",
            }),
        }]),
        answer_step("this step must never run"),
    ]);

    let mut sink = RecordingSink::new();
    let err = run_turn(turn("edit the note", context), &provider, &mut sink)
        .await
        .expect_err("a lost authority must end the turn");
    assert!(
        matches!(err, TurnError::ToolAuthorityRevoked { .. }),
        "{err}"
    );
    assert_eq!(
        kinds(&sink),
        [
            "tool_start",
            "tool_result",
            "tool_start",
            "tool_result",
            "error"
        ]
    );
    assert_eq!(
        provider.requests().len(),
        2,
        "the turn asked the model for another step after losing an authority"
    );
    assert_eq!(tree.read("notes.md"), "swapped\n");
}

// ---------------------------------------------------------------------------
// binary-level acceptance
// ---------------------------------------------------------------------------

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        Self {
            home: home.canonicalize().expect("canonicalize home"),
            workspace: workspace.canonicalize().expect("canonicalize workspace"),
            _root: root,
        }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.workspace.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(&path, contents).expect("write the fixture file");
        path
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.workspace.join(relative)).expect("read the file")
    }

    /// Spawns fxr with its stdout on a pipe, for a test that has to act on the
    /// process while it is still running.
    fn spawn(&self, args: &[&str], env: &[(&str, &str)]) -> Child {
        let mut command = self.command(args, env);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.spawn().expect("spawn fxr")
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        Run::of(self.command(args, env).output().expect("spawn fxr"))
    }

    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fxr"));
        command.current_dir(&self.workspace);
        command.env("HOME", &self.home);
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        command.args(args);
        command
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).expect("stdout is utf-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr is utf-8"),
        }
    }

    fn events(&self) -> Vec<Value> {
        assert!(
            self.stdout.is_empty() || self.stdout.ends_with('\n'),
            "JSONL must be newline terminated, got {:?}",
            self.stdout
        );
        self.stdout
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|err| panic!("`{line}` is not JSON ({err})"))
            })
            .collect()
    }

    fn kinds(&self) -> Vec<String> {
        self.events()
            .iter()
            .map(|event| event["kind"].as_str().expect("a kind").to_string())
            .collect()
    }

    fn assert_no_secret(&self) {
        assert!(!self.stdout.contains(TEST_KEY), "the key reached stdout");
        assert!(!self.stderr.contains(TEST_KEY), "the key reached stderr");
    }
}

#[test]
fn ask_advertises_the_permission_mode_flags() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "--help"], &[]);
    assert_eq!(run.code, Some(0));
    assert!(run.stdout.contains("--auto"), "{}", run.stdout);
    assert!(run.stdout.contains("--yolo"), "{}", run.stdout);
}

#[test]
fn status_reports_the_permission_mode_the_flags_selected() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["status", "--json"], &[("FXR_PERMISSION_MODE", "ask")]);
    assert_eq!(run.code, Some(0));
    let document: Value = serde_json::from_str(run.stdout.trim()).expect("one JSON document");
    assert_eq!(document["permission_mode"], "ask");
    assert_eq!(document["sandbox"], "none");
}

#[test]
fn doctor_says_what_the_current_mode_will_do_without_being_asked() {
    let sandbox = Sandbox::new();
    let check = |mode: &str| -> Value {
        let run = sandbox.run(&["doctor", "--json"], &[("FXR_PERMISSION_MODE", mode)]);
        assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
        let document: Value = serde_json::from_str(run.stdout.trim()).expect("one JSON document");
        document["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .find(|check| check["name"] == "permissions")
            .expect("doctor must report a `permissions` check")
            .clone()
    };

    // `ask` with no terminal is the confusing case: it is working correctly and
    // refusing everything, so `doctor` has to say so out loud.
    let asking = check("ask");
    assert_eq!(asking["status"], "warn");
    let detail = asking["detail"].as_str().expect("detail");
    assert!(detail.contains("no terminal"), "{detail}");
    assert!(detail.contains("refused"), "{detail}");

    let automatic = check("auto");
    assert_eq!(automatic["status"], "ok");
    assert!(
        automatic["detail"]
            .as_str()
            .expect("detail")
            .contains("sandbox=none"),
        "{automatic}"
    );

    let reckless = check("yolo");
    assert_eq!(reckless["status"], "warn");
    assert!(
        reckless["detail"]
            .as_str()
            .expect("detail")
            .contains("no permission check"),
        "{reckless}"
    );
}

#[test]
fn yolo_prints_a_visible_warning_on_stderr_before_it_runs_anything() {
    let gateway = FakeGateway::start(vec![Reply::Sse(sse_body(&[
        text_delta("a0", "done"),
        finish("stop"),
    ]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--yolo", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("FXR_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert!(run.stderr.contains(YOLO_WARNING), "{}", run.stderr);
    // The warning is a diagnostic, so it must not corrupt a JSONL pipe.
    assert_eq!(run.kinds(), ["assistant_delta", "final"]);
    run.assert_no_secret();
}

#[test]
fn ask_mode_without_a_terminal_refuses_the_edit_and_leaves_the_file_alone() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("c1", "read_file", json!({ "path": "notes.md" })),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            tool_call(
                "c2",
                "edit_file",
                json!({
                    "path": "notes.md",
                    "old_string": "original",
                    "new_string": "edited",
                }),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            text_delta("a0", "I need your approval"),
            finish("stop"),
        ])),
    ]);
    let sandbox = Sandbox::new();
    sandbox.write("notes.md", "original\n");
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "edit", "the", "note"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("FXR_GATEWAY_URL", &gateway.chat_url()),
            ("FXR_PERMISSION_MODE", "ask"),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    let events = run.events();
    let edit = events
        .iter()
        .find(|event| event["tool"] == "edit_file" && event["kind"] == "tool_result")
        .expect("the edit produced a result");
    assert_eq!(edit["ok"], json!(false));
    assert_eq!(sandbox.read("notes.md"), "original\n");
    run.assert_no_secret();
}

#[test]
fn an_interrupt_stops_a_running_command_and_ends_the_turn_as_cancelled() {
    // Cancellation existed in the types before this test and was unreachable
    // from the binary: nothing ever set the token. Here a real SIGINT reaches a
    // real process that is really blocked in `sleep 30`.
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call(
                "c1",
                "terminal",
                json!({ "action": "exec", "command": "sleep 30" }),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            text_delta("a0", "this step must never run"),
            finish("stop"),
        ])),
    ]);
    let sandbox = Sandbox::new();
    let mut child = sandbox.spawn(
        &["ask", "--yolo", "--json", "--no-save", "wait", "for", "me"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("FXR_GATEWAY_URL", &gateway.chat_url()),
        ],
    );

    let mut stdout = child.stdout.take().expect("piped stdout");
    let collected = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&collected);
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        *sink.lock().expect("stdout") = text;
    });

    // The first Gateway request proves fxr is past startup, so the interrupt
    // handler is installed and SIGINT will not fall through to the default
    // disposition.
    let ready = Instant::now() + Duration::from_secs(30);
    while gateway.request_count() < 1 && Instant::now() < ready {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(gateway.request_count(), 1, "fxr never reached the Gateway");
    std::thread::sleep(Duration::from_millis(500));

    let pid = rustix::process::Pid::from_raw(child.id() as i32).expect("a live pid");
    rustix::process::kill_process(pid, rustix::process::Signal::INT).expect("send SIGINT");

    let started = Instant::now();
    let deadline = started + Duration::from_secs(25);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait on fxr") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "fxr ignored SIGINT and was still running after {:?}",
            started.elapsed()
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let _ = reader.join();

    // Well before `sleep 30` would have ended on its own.
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the interrupt took {:?}",
        started.elapsed()
    );
    assert_eq!(
        status.code(),
        Some(1),
        "an interrupted turn did not complete"
    );

    let text = collected.lock().expect("stdout").clone();
    let events: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("`{line}`: {err}")))
        .collect();
    let last = events.last().expect("a terminal event");
    assert_eq!(last["kind"], "error", "{text}");
    assert!(
        last["message"]
            .as_str()
            .expect("a message")
            .contains("cancelled"),
        "{text}"
    );
    // The second model step never happened: the turn stopped rather than
    // carrying on after the command was killed.
    assert_eq!(
        gateway.request_count(),
        1,
        "the turn continued past the interrupt"
    );
}

#[test]
fn the_release_loop_reads_a_file_edits_it_runs_a_command_and_answers() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("c1", "read_file", json!({ "path": "notes.md" })),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            tool_call(
                "c2",
                "edit_file",
                json!({
                    "path": "notes.md",
                    "old_string": "before",
                    "new_string": "after",
                }),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            tool_call(
                "c3",
                "terminal",
                json!({ "action": "exec", "command": "cat notes.md" }),
            ),
            finish("tool-calls"),
        ])),
        // The only Cargo invocation `auto` still admits: it reports and it
        // cannot execute the workspace it was just allowed to write to.
        Reply::Sse(sse_body(&[
            tool_call(
                "c4",
                "terminal",
                json!({ "action": "exec", "command": "cargo --version" }),
            ),
            finish("tool-calls"),
        ])),
        // ... and the one it does not, so the release loop shows both answers.
        Reply::Sse(sse_body(&[
            tool_call(
                "c5",
                "terminal",
                json!({ "action": "exec", "command": "cargo test" }),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            text_delta("a0", "changed before to after"),
            finish("stop"),
        ])),
    ]);
    let sandbox = Sandbox::new();
    sandbox.write("notes.md", "the word before is here\n");

    let run = sandbox.run(
        &[
            "ask",
            "--auto",
            "--json",
            "--no-save",
            "rename",
            "the",
            "word",
        ],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("FXR_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    // Every tool ran exactly once, in order, each with one correlated result.
    let events = run.events();
    let tools: Vec<&str> = events
        .iter()
        .filter(|event| event["kind"] == "tool_start")
        .map(|event| event["tool"].as_str().expect("a tool"))
        .collect();
    assert_eq!(
        tools,
        ["read_file", "edit_file", "terminal", "terminal", "terminal"]
    );
    assert_eq!(
        run.kinds().last().map(String::as_str),
        Some("final"),
        "{}",
        run.stdout
    );

    let result_for = |call: &str| -> Value {
        events
            .iter()
            .find(|event| event["kind"] == "tool_result" && event["call_id"] == call)
            .unwrap_or_else(|| panic!("no result for {call}"))
            .clone()
    };
    for call in ["c1", "c2", "c3", "c4"] {
        assert_eq!(result_for(call)["ok"], json!(true), "{call}");
    }
    // `cargo test` compiles and runs the workspace `auto` may write to, so it is
    // the one call in this loop that has to be refused.
    let denied = result_for("c5");
    assert_eq!(denied["ok"], json!(false));
    assert!(
        denied["detail"]
            .as_str()
            .expect("a detail")
            .contains("not admitted in auto mode"),
        "{denied}"
    );

    // The file changed once, to exactly the requested content.
    assert_eq!(sandbox.read("notes.md"), "the word after is here\n");

    // The terminal saw the edited file, so the exec really ran after the write.
    let requests = gateway.requests();
    assert_eq!(requests.len(), 6, "one request per model step");
    let last = requests[5].json();
    let exec_result = last["prompt"]
        .as_array()
        .expect("a prompt")
        .iter()
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .find(|part| part["type"] == "tool-result" && part["toolCallId"] == "c3")
        .expect("the exec result is correlated in the prompt");
    let value = exec_result["output"]["value"].as_str().expect("text");
    assert!(value.contains("the word after is here"), "{value}");
    assert!(value.contains("<exit_code>0</exit_code>"), "{value}");

    // Nothing was left behind and the workspace holds only what it started with.
    let mut entries: Vec<String> = fs::read_dir(&sandbox.workspace)
        .expect("read the workspace")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    assert_eq!(entries, ["notes.md"]);
    run.assert_no_secret();
}
