//! `/mcp`'s verbs, sentence by sentence: what each says, in which tone, and
//! what each writes. Every file a case writes is in a throwaway repository
//! under the run's temporary directory, never the checkout.

use super::*;
use crate::mcp::runtime::{Event, Paths};

fn words(line: &str) -> Vec<String> {
    tokenize(line).expect("a line")
}

/// What `/mcp <line>` said: each notice, `!` before a warning and `*` before
/// a neutral one, as the transcript marks them.
fn said(runtime: &Arc<McpRuntime>, line: &str) -> Vec<String> {
    handle(runtime, line);
    runtime
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            Event::Notice { warning, body } => {
                Some(format!("{} {body}", if warning { "!" } else { "*" }))
            }
            _ => None,
        })
        .collect()
}

fn usage_of(line: &str) -> String {
    match parse(&words(line)) {
        Parsed::Usage { message, .. } => message,
        other => panic!("{line}: {other:?}"),
    }
}

/// A home and a workspace inside a throwaway repository under the run's
/// temporary directory.
struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("dir");
        let checkout = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        assert!(!root.starts_with(&checkout), "{root:?}");
        assert!(
            root.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()),
            "{root:?}"
        );
        let work = root.join("work");
        std::fs::create_dir_all(work.join(".git")).unwrap();
        std::fs::write(work.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        Self { _dir: dir, root }
    }

    fn profile(&self) -> PathBuf {
        self.root.join(".xfx").join("mcp.json")
    }

    fn project(&self) -> PathBuf {
        self.root.join("work").join(".mcp.json")
    }

    fn settings(&self) -> PathBuf {
        self.root.join(".xfx").join("settings.json")
    }

    fn write_profile(&self, text: &str) {
        std::fs::create_dir_all(self.profile().parent().unwrap()).unwrap();
        std::fs::write(self.profile(), text).unwrap();
    }

    /// A runtime over this home, its startup load done.
    fn runtime(&self) -> Arc<McpRuntime> {
        let runtime = McpRuntime::new(Paths {
            profile: Some(self.profile()),
            settings: Some(self.settings()),
            workspace: self.root.join("work"),
        });
        runtime.start_and_wait(Duration::from_secs(60));
        runtime.take_events();
        runtime
    }
}

fn fixture() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/mcp_fixture.sh")
        .to_string_lossy()
        .into_owned()
}

/// Waits until `name` is up, or fails the case.
fn ready(runtime: &McpRuntime, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !runtime
        .views()
        .iter()
        .any(|view| view.name == name && view.connection == Connection::Ready)
    {
        assert!(Instant::now() < deadline, "{name} never came up");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_line_is_split_as_fxs_lexer_splits_it() {
    assert_eq!(words("  show   fixture "), ["show", "fixture"]);
    assert_eq!(
        words(r#"add web https://w --header "Authorization=Bearer a b""#),
        [
            "add",
            "web",
            "https://w",
            "--header",
            "Authorization=Bearer a b"
        ]
    );
    assert_eq!(words("a 'b c' d"), ["a", "b c", "d"]);
    assert_eq!(words(r#"a "x\$y\q" 'it\s'"#), ["a", "x$y\\q", "it\\s"]);
    assert_eq!(words(r"a\ b"), ["a b"]);
    assert_eq!(words("''"), [""]);
    // An operator is a word of its own, as fx's lexer makes it.
    assert_eq!(words("a|b;c&&d"), ["a", "|", "b", ";", "c", "&&", "d"]);
    assert_eq!(words("a>b 2&>c"), ["a", ">", "b", "2", "&>", "c"]);
    assert_eq!(words("'a|b'"), ["a|b"]);
    assert_eq!(words("X=${K}"), ["X=$", "{", "K", "}"]);
    assert_eq!(words(""), Vec::<String>::new());
    // Refused: fx's `UnbalancedQuote`, `MalformedEscape`, `EmbeddedNul`.
    for refused in ["show 'x", "show \"x", "show x\\", "show a\0b"] {
        assert_eq!(tokenize(refused), None, "{refused:?}");
    }
}

#[test]
fn every_mistake_is_fxs_sentence() {
    // fx's own cases (`verbs.zig:934-967@481104fd`), for `/mcp`, and the
    // ones `p5-mcp-empty` and `p5-mcp-verbs` type.
    for (line, message) in [
        ("nope", "'nope' isn't a /mcp command."),
        ("show", "/mcp show needs one server NAME."),
        ("show a b", "/mcp show needs one server NAME."),
        (
            "add 'bad name' https://w",
            "'bad name' can't be a server name: use letters, digits, '-' and '_'.",
        ),
        (
            "add bad.name http://127.0.0.1:9/x",
            "'bad.name' can't be a server name: use letters, digits, '-' and '_'.",
        ),
        (
            "add web w.example",
            "'w.example' isn't a URL. A stdio server's command goes after --.",
        ),
        ("add web", "/mcp add needs a URL, or -- and a command."),
        ("add", "/mcp add needs a NAME."),
        (
            "add three /bin/x a",
            "/mcp add needs a URL, or -- and a command.",
        ),
        ("add web --", "/mcp add needs a command after --."),
        (
            "add web https://w -- x",
            "/mcp add takes a URL or a command after --, not both.",
        ),
        (
            "add --env A=1 web https://w",
            "--env is for stdio servers; use --header for HTTP.",
        ),
        (
            "add --header A=1 web -- node",
            "--header is for HTTP servers; use --env for a command.",
        ),
        ("add --env noequals web -- node", "--env needs KEY=VALUE."),
        ("add --env =x web -- node", "--env needs KEY=VALUE."),
        ("add --header", "--header needs KEY=VALUE."),
        (
            "add --env A=1 --header B=2 web -- node",
            "--env is for stdio servers and --header for HTTP ones.",
        ),
        (
            "add --transport http web http://127.0.0.1:9/web",
            "/mcp add has no --transport flag.",
        ),
        (
            "remove --project --profile x",
            "Use --project or --profile, not both.",
        ),
        (
            "approve --all x",
            "/mcp approve takes a NAME or --all, not both.",
        ),
        ("logout --json x", "/mcp logout has no --json flag."),
        ("show x -- y", "/mcp show takes no command after --."),
        ("list --json", "/mcp list has no --json flag."),
        ("list fixture", "/mcp list takes no server name."),
        ("approve", "/mcp approve needs one server NAME."),
        ("reject", "/mcp reject needs one server NAME."),
        ("reject --all", "/mcp reject has no --all flag."),
        ("login", "/mcp login needs one server NAME."),
        ("login --open x", "/mcp login has no --open flag."),
        ("logout", "/mcp logout needs one server NAME."),
        ("remove", "/mcp remove needs one server NAME."),
        ("remove --all x", "/mcp remove has no --all flag."),
    ] {
        assert_eq!(usage_of(line), message, "{line}");
    }
    // A word someone typed is said back spelled.
    assert_eq!(usage_of("'\u{1b}[2J'"), "'\\x1b[2J' isn't a /mcp command.");
    assert_eq!(
        usage_of("show --\u{202e}x"),
        "/mcp show has no --\\u{202e}x flag."
    );
}

#[test]
fn every_verb_fx_v1_had_says_where_it_went() {
    // `fx-p5-mcp-empty-40x120-end.full` rows 20-30 and 56, `fx` read `xfx`.
    for (line, message) in [
        ("auth x", "/mcp auth is now /mcp login NAME."),
        ("auth", "/mcp auth is now /mcp login NAME."),
        (
            "trust approve x",
            "/mcp trust is now /mcp approve NAME and /mcp reject NAME.",
        ),
        (
            "trust",
            "/mcp trust is now /mcp approve NAME and /mcp reject NAME.",
        ),
        (
            "path",
            "/mcp path is gone: /mcp list shows each server's file.",
        ),
        (
            "reload",
            "/mcp reload is gone: xfx reloads after add, remove, approve, and reject.",
        ),
        (
            "resource list x",
            "/mcp resource is gone: xfx doesn't support MCP resources and prompts yet.",
        ),
        (
            "prompt",
            "/mcp prompt is gone: xfx doesn't support MCP resources and prompts yet.",
        ),
        (
            "list --connect",
            "/mcp list --connect is now /mcp show NAME.",
        ),
    ] {
        assert_eq!(parse(&words(line)), Parsed::Moved(message.into()), "{line}");
    }
}

#[test]
fn every_verb_reads_its_flags_before_the_dash_and_its_command_after_it() {
    assert_eq!(parse(&[]), Parsed::Command(Command::Help(None)));
    for help in ["help", "-h", "--help"] {
        assert_eq!(parse(&words(help)), Parsed::Command(Command::Help(None)));
    }
    assert_eq!(
        parse(&words("add -h")),
        Parsed::Command(Command::Help(Some(Verb::Add)))
    );
    assert_eq!(
        parse(&words("approve --help x")),
        Parsed::Command(Command::Help(Some(Verb::Approve)))
    );
    assert_eq!(parse(&words("list")), Parsed::Command(Command::List));
    assert_eq!(
        // `${K}` quoted: unquoted, its braces are words of their own, as fx's
        // lexer splits them.
        parse(&words(
            "add web https://w/mcp --header 'X-Key=${K}' --project"
        )),
        Parsed::Command(Command::Add {
            name: "web".into(),
            file: File::Project,
            entry: Entry::Http {
                url: "https://w/mcp".into(),
                headers: vec![("X-Key".into(), "${K}".into())],
            },
        })
    );
    assert_eq!(
        parse(&words("add --env A=1=2 local -- node server.js --help -h")),
        Parsed::Command(Command::Add {
            name: "local".into(),
            file: File::Profile,
            entry: Entry::Stdio {
                command: "node".into(),
                args: vec!["server.js".into(), "--help".into(), "-h".into()],
                env: vec![("A".into(), "1=2".into())],
            },
        })
    );
    // Slack's server needs no URL; with one it is any other server.
    assert_eq!(
        parse(&words("add slack")),
        Parsed::Command(Command::AddSlack)
    );
    assert_eq!(
        parse(&words("add --project slack")),
        Parsed::Command(Command::AddSlack)
    );
    assert!(matches!(
        parse(&words("add slack https://slack.example/mcp")),
        Parsed::Command(Command::Add { .. })
    ));
    assert_eq!(
        parse(&words("remove --project x")),
        Parsed::Command(Command::Remove {
            name: "x".into(),
            file: Some(File::Project)
        })
    );
    assert_eq!(
        parse(&words("remove --profile x")),
        Parsed::Command(Command::Remove {
            name: "x".into(),
            file: Some(File::Profile)
        })
    );
    assert_eq!(
        parse(&words("remove x")),
        Parsed::Command(Command::Remove {
            name: "x".into(),
            file: None
        })
    );
    assert_eq!(
        parse(&words("login --no-browser x")),
        Parsed::Command(Command::Login("x".into()))
    );
    assert_eq!(
        parse(&words("logout x")),
        Parsed::Command(Command::Logout("x".into()))
    );
    assert_eq!(
        parse(&words("approve --all")),
        Parsed::Command(Command::Approve(None))
    );
    assert_eq!(
        parse(&words("approve x")),
        Parsed::Command(Command::Approve(Some("x".into())))
    );
    assert_eq!(
        parse(&words("reject x")),
        Parsed::Command(Command::Reject("x".into()))
    );
    assert_eq!(
        parse(&words("show x")),
        Parsed::Command(Command::Show("x".into()))
    );
}

#[test]
fn the_overview_and_each_usage_are_fxs_rows() {
    // `fx-p5-mcp-empty-40x120-end.full`, rows 3-16 and 45-54, with
    // `~/.fx/mcp.json` spelled `~/.xfx/mcp.json`.
    assert_eq!(
        usage(None),
        "usage: /mcp COMMAND\n\n\
         \x20 list                        configured servers; never connects\n\
         \x20 show NAME                   connect to NAME: status, version, tools\n\
         \x20 add NAME URL                add an HTTP server\n\
         \x20 add NAME -- CMD [ARGS...]   add a stdio server\n\
         \x20 add slack                   add Slack's server\n\
         \x20 remove NAME                 remove a server and its login\n\
         \x20 login NAME                  sign in to an HTTP server\n\
         \x20 logout NAME                 sign out and delete the login\n\
         \x20 approve NAME | --all        approve project servers from .mcp.json\n\
         \x20 reject NAME                 reject a project server\n\
         \nRun '/mcp COMMAND -h' for a command's flags.\n"
    );
    assert_eq!(
        usage(Some(Verb::Add)),
        "usage: /mcp add [--project] [--header K=V]... NAME URL\n\
         \x20      /mcp add [--project] [--env K=V]... NAME -- COMMAND [ARGS...]\n\
         \x20      /mcp add [--project] slack\n\
         \x20 --project  write ./.mcp.json instead of ~/.xfx/mcp.json\n"
    );
    for (verb, text) in [
        (Verb::List, "usage: /mcp list\n"),
        (Verb::Show, "usage: /mcp show NAME\n"),
        (Verb::Remove, "usage: /mcp remove [--project | --profile] NAME\n"),
        (
            Verb::Login,
            "usage: /mcp login [--no-browser] NAME\n  --no-browser  print the address instead of opening it\n",
        ),
        (Verb::Logout, "usage: /mcp logout NAME\n"),
        (Verb::Approve, "usage: /mcp approve NAME | --all\n"),
        (Verb::Reject, "usage: /mcp reject NAME\n"),
    ] {
        assert_eq!(usage(Some(verb)), text, "{verb:?}");
    }
}

#[test]
fn with_no_servers_every_verb_answers_in_fxs_words_and_tone() {
    // `fx-p5-mcp-empty-40x120-end.full`, notice by notice: a usage mistake
    // and a refusal are warnings, an answer and a usage asked for neutral.
    let scratch = Scratch::new();
    let runtime = scratch.runtime();
    let overview = usage(None);
    for line in ["help", "-h", "--help", ""] {
        assert_eq!(
            said(&runtime, line),
            [format!("* {}", overview.trim_end())],
            "{line}"
        );
    }
    assert_eq!(
        said(&runtime, "list"),
        ["* No MCP servers configured. Add one with: /mcp add NAME URL"]
    );
    for (line, answer) in [
        (
            "show nope",
            "! No MCP server named nope. /mcp list shows them.",
        ),
        (
            "login nope",
            "! No MCP server named nope. /mcp list shows them.",
        ),
        (
            "logout nope",
            "! No MCP server named nope. /mcp list shows them.",
        ),
        (
            "remove nope",
            "! No MCP server named nope in ~/.xfx/mcp.json or .mcp.json.",
        ),
        (
            "remove --project nope",
            "! No MCP server named nope in .mcp.json.",
        ),
        (
            "remove --profile nope",
            "! No MCP server named nope in ~/.xfx/mcp.json.",
        ),
        (
            "reject nope",
            "! No project server named nope in .mcp.json.",
        ),
        (
            "approve nope",
            "! No project server named nope in .mcp.json.",
        ),
        (
            "approve --all",
            "! No project servers are waiting for approval.",
        ),
        (
            "add slack",
            "! Interactive MCP authentication is unavailable here.",
        ),
        ("path", "! /mcp path is gone: /mcp list shows each server's file."),
        (
            "show 'x",
            "! /mcp: that command line has an unmatched quote or a character xfx doesn't take here.",
        ),
    ] {
        assert_eq!(said(&runtime, line), [answer], "{line}");
    }
    assert_eq!(
        said(&runtime, "approve"),
        ["! /mcp approve needs one server NAME.\nusage: /mcp approve NAME | --all"]
    );
    assert_eq!(
        said(&runtime, "add"),
        [format!(
            "! /mcp add needs a NAME.\n{}",
            usage(Some(Verb::Add)).trim_end()
        )]
    );
    assert_eq!(
        said(&runtime, "add -h"),
        [format!("* {}", usage(Some(Verb::Add)).trim_end())]
    );
    assert_eq!(
        said(&runtime, "bogus"),
        [format!(
            "! 'bogus' isn't a /mcp command.\n{}",
            overview.trim_end()
        )]
    );
    // A gated `add slack` wrote nothing.
    assert!(!scratch.profile().exists());
    assert!(!scratch.project().exists());
}

#[test]
fn add_and_remove_write_through_the_config_writer_and_say_fxs_sentences() {
    let scratch = Scratch::new();
    let runtime = scratch.runtime();
    assert_eq!(
        said(&runtime, "add web2 http://127.0.0.1:9/web2"),
        ["* Added web2 to ~/.xfx/mcp.json."]
    );
    assert_eq!(
        std::fs::read_to_string(scratch.profile()).unwrap(),
        "{\n  \"mcpServers\": {\n    \"web2\": {\n      \"type\": \"http\",\n      \"url\": \"http://127.0.0.1:9/web2\"\n    }\n  }\n}\n"
    );
    // The reload after it put the server in force.
    assert!(runtime.server("web2").is_some());
    assert_eq!(
        said(&runtime, "add web2 http://127.0.0.1:9/again"),
        ["! Couldn't add web2 to ~/.xfx/mcp.json: that name is already in the file; remove it first."]
    );
    // A project add edits the workspace's `.mcp.json`, and says the
    // profile's of the same name is the one used.
    assert_eq!(
        said(&runtime, "add --project web2 -- /bin/true"),
        ["* Added web2 to .mcp.json.\nweb2 is also in ~/.xfx/mcp.json; the one in ~/.xfx/mcp.json is used."]
    );
    assert_eq!(
        std::fs::read_to_string(scratch.project()).unwrap(),
        "{\n  \"mcpServers\": {\n    \"web2\": {\n      \"command\": \"/bin/true\"\n    }\n  }\n}\n"
    );
    assert_eq!(
        said(&runtime, "remove web2"),
        ["! web2 is in both ~/.xfx/mcp.json and .mcp.json. Add --profile or --project."]
    );
    assert_eq!(
        said(&runtime, "remove --project web2"),
        ["* Removed web2 from .mcp.json."]
    );
    assert_eq!(
        said(&runtime, "remove web2"),
        ["* Removed web2 from ~/.xfx/mcp.json."]
    );
    assert!(runtime.server("web2").is_none());
    assert_eq!(
        said(&runtime, "remove web2"),
        ["! No MCP server named web2 in ~/.xfx/mcp.json or .mcp.json."]
    );
    // A file the writer can't read is refused in fx's words, nothing written.
    std::fs::write(scratch.project(), "{not json").unwrap();
    assert_eq!(
        said(&runtime, "add --project p -- /bin/true"),
        ["! Couldn't add p to .mcp.json: the file isn't valid JSON, or repeats a key; fix it by hand first."]
    );
    assert_eq!(
        std::fs::read_to_string(scratch.project()).unwrap(),
        "{not json"
    );
    runtime.shutdown();
}

#[test]
fn a_header_or_env_value_is_never_said_back_and_never_planted_in_the_project() {
    let scratch = Scratch::new();
    let runtime = scratch.runtime();
    let secret = "s3cr3t-token-value";
    // `Authorization` in a project's file, in any case: refused before
    // anything is written, and the value is not in the sentence.
    for header in ["Authorization", "authorization", "AUTHORIZATION"] {
        let answer = said(
            &runtime,
            &format!("add --project --header '{header}=Bearer {secret}' web https://w.example/mcp"),
        );
        assert_eq!(
            answer,
            ["! Couldn't add web to .mcp.json: xfx won't send an Authorization header from .mcp.json; add the server without --project to keep it in ~/.xfx/mcp.json."],
            "{header}"
        );
        assert!(!scratch.project().exists(), "{header}: .mcp.json written");
    }
    // The profile takes it, and its sentence does not carry the value.
    let answer = said(
        &runtime,
        &format!("add --header 'Authorization=Bearer {secret}' web https://w.example/mcp"),
    );
    assert_eq!(answer, ["* Added web to ~/.xfx/mcp.json."]);
    let answer = said(
        &runtime,
        &format!("add --project --env 'TOKEN={secret}' local -- /bin/true"),
    );
    assert_eq!(answer, ["* Added local to .mcp.json."]);
    for line in [
        format!("add --env 'TOKEN={secret}' local2 https://w.example/mcp"),
        format!("add --header 'X={secret}' local3 -- /bin/true"),
        format!("add --header 'X={secret}' web https://w.example/mcp"),
        format!("add --header 'X={secret}' 'bad name' https://w.example/mcp"),
        // MCP-Vb: the `=` forms are no flag of `add`'s, and the refusal said
        // the whole word back, value and all.
        format!("add --env=TOKEN={secret} local -- /bin/true"),
        format!("add --project --header=Authorization={secret} web https://w.example/mcp"),
        format!("add --header=X={secret} web https://w.example/mcp"),
        format!("show --token={secret} fixture"),
        format!("add TOKEN={secret} https://w.example/mcp"),
        format!("add web w.example/?token={secret}"),
        format!("TOKEN={secret}"),
        format!("show TOKEN={secret}"),
        format!("remove TOKEN={secret}"),
        format!("approve TOKEN={secret}"),
        format!("login TOKEN={secret}"),
    ] {
        let answer = said(&runtime, &line);
        assert!(
            !answer.iter().any(|text| text.contains(secret)),
            "{answer:?}"
        );
    }
    runtime.shutdown();
}

#[test]
fn a_header_the_reader_would_refuse_is_refused_before_anything_is_written() {
    // MCP-Vb: a padded `Authorization` passed the exact-name refusal and was
    // written into the repository's `.mcp.json`, which the reader then
    // refused -- with the token already in a shared file.
    let scratch = Scratch::new();
    let runtime = scratch.runtime();
    let secret = "padded-header-secret";
    for header in [
        format!("' Authorization={secret}'"),
        format!("'Authorization ={secret}'"),
        format!("'X Key={secret}'"),
        format!("'Content-Type={secret}'"),
    ] {
        let answer = said(
            &runtime,
            &format!("add --project --header {header} web https://w.example/mcp"),
        );
        assert_eq!(
            answer,
            ["! Couldn't add web to .mcp.json: its \"headers\" isn't valid."],
            "{header}"
        );
        assert!(!scratch.project().exists(), "{header}: .mcp.json written");
    }
    // The profile's reader holds headers to the same rules.
    let answer = said(
        &runtime,
        &format!("add --header ' Authorization={secret}' web https://w.example/mcp"),
    );
    assert_eq!(
        answer,
        ["! Couldn't add web to ~/.xfx/mcp.json: its \"headers\" isn't valid."]
    );
    assert!(!scratch.profile().exists());
    // So is a URL: `add` never says `Added` for an entry the reader skips.
    for url in [
        "http://example.test/mcp",
        "https://user:pw@example.test/mcp",
        "https://example.test/mcp#x",
    ] {
        let answer = said(&runtime, &format!("add web {url}"));
        assert_eq!(
            answer,
            ["! Couldn't add web to ~/.xfx/mcp.json: its \"url\" isn't valid."],
            "{url}"
        );
    }
    assert!(!scratch.profile().exists());
    // A `${...}` is the reader's to expand, as it does once an entry is
    // approved: left as written.
    assert_eq!(
        said(&runtime, "add --project web 'https://${XFX_MCPV_HOST}/mcp'"),
        ["* Added web to .mcp.json."]
    );
    runtime.shutdown();
}

#[test]
fn approve_reaches_an_entry_that_parses_only_once_expanded() {
    // MCP-Vb: `"type": "${T:-stdio}"` is a configuration error until it is
    // expanded, which only an approved entry is, so it has no server to find
    // by name; `approve NAME` is the one way to approve it, for the entry as
    // written -- shown, and bound to its digest.
    let scratch = Scratch::new();
    scratch.write_profile("{}");
    std::fs::write(
        scratch.project(),
        format!(
            r#"{{"mcpServers":{{"late":{{"type":"${{XFX_MCPV_UNSET_TYPE:-stdio}}","command":"/bin/sh","args":["{}"]}}}}}}"#,
            fixture()
        ),
    )
    .unwrap();
    let runtime = scratch.runtime();
    assert!(runtime.server("late").is_none());
    assert_eq!(
        said(&runtime, "approve late"),
        [format!(
            "* Approved late, which runs: /bin/sh {}",
            fixture()
        )]
    );
    ready(&runtime, "late");
    assert_eq!(
        said(&runtime, "approve nope"),
        ["! No project server named nope in .mcp.json."]
    );
    runtime.shutdown();
}

#[test]
fn show_and_list_never_print_an_env_or_header_value_raw_or_expanded() {
    // fx's `show` prints a stdio server's command line -- expanded, once a
    // profile's `${VAR}` is (`src/core/mcp_host/verbs.zig:528@481104fd`) --
    // and an HTTP server's URL, and never a variable or a header it sends.
    // Each value here is a `${VAR:-default}` whose variable is unset, so a
    // reader that expands a profile leaves the default and one that does not
    // leaves the text as written -- neither drops the server -- and a plain
    // value stands for one expanded already: the case holds either way.
    let scratch = Scratch::new();
    let (env_raw, env_expanded) = (
        "${XFX_MCPV_TEST_UNSET:-expanded-env-secret}",
        "expanded-env-secret",
    );
    let (header_raw, header_expanded) = (
        "${XFX_MCPV_TEST_UNSET:-expanded-header-secret}",
        "expanded-header-secret",
    );
    let (env_plain, header_plain) = ("plain-env-secret", "plain-header-secret");
    scratch.write_profile(&format!(
        r#"{{"mcpServers":{{"fixture":{{"command":"/bin/sh","args":["{}"],"env":{{"TOKEN":"{env_raw}","PLAIN":"{env_plain}"}}}},"remote":{{"type":"http","url":"http://127.0.0.1:9/mcp","headers":{{"X-Key":"{header_raw}","X-Plain":"{header_plain}"}}}}}}}}"#,
        fixture()
    ));
    let runtime = scratch.runtime();
    ready(&runtime, "fixture");
    let mut answers: Vec<String> = Vec::new();
    for line in [
        "list",
        "show fixture",
        "show remote",
        "login remote",
        "logout remote",
    ] {
        answers.extend(said(&runtime, line));
    }
    // The case read the servers it set up: their rows are there.
    let all = answers.join("\n");
    assert!(
        all.contains("    echo  Echo text back through the test fixture"),
        "{all}"
    );
    assert!(all.contains("  url        http://127.0.0.1:9/mcp"), "{all}");
    for secret in [
        env_raw,
        env_expanded,
        env_plain,
        header_raw,
        header_expanded,
        header_plain,
    ] {
        assert!(!all.contains(secret), "{secret} said back:\n{all}");
    }
    runtime.shutdown();
}

#[cfg(unix)]
#[test]
fn a_link_where_the_project_file_should_be_is_refused_in_fxs_words() {
    let scratch = Scratch::new();
    let runtime = scratch.runtime();
    let outside = scratch.root.join("outside.json");
    std::fs::write(&outside, "{}").unwrap();
    std::os::unix::fs::symlink(&outside, scratch.project()).unwrap();
    assert_eq!(
        said(&runtime, "add --project p -- /bin/true"),
        [format!(
            "! Couldn't add p to .mcp.json: it's a link to {}, and xfx edits only a plain file; edit {} by hand.",
            outside.display(),
            outside.display()
        )]
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "{}");
    runtime.shutdown();
}

#[test]
fn list_show_login_and_logout_over_a_stdio_a_remote_and_a_project_server() {
    let scratch = Scratch::new();
    scratch.write_profile(&format!(
        r#"{{"mcpServers":{{"fixture":{{"command":"/bin/sh","args":["{}"]}},"remote":{{"type":"http","url":"http://127.0.0.1:9/mcp","enabled":false}},"broken":{{"command":123}}}}}}"#,
        fixture()
    ));
    std::fs::write(
        scratch.project(),
        r#"{"mcpServers":{"proj":{"command":"/bin/true","args":["a b"]}}}"#,
    )
    .unwrap();
    let runtime = scratch.runtime();
    ready(&runtime, "fixture");
    let note = "! note: ~/.xfx/mcp.json server 'broken' was skipped: it needs a \"command\", with any \"args\" as strings.";
    assert_eq!(
        said(&runtime, "list"),
        [
            "* NAME     SOURCE           TRANSPORT  STATUS\n\
             fixture  ~/.xfx/mcp.json  stdio      ready, 1 tool\n\
             remote   ~/.xfx/mcp.json  http       disabled\n\
             proj     .mcp.json        stdio      waiting for approval",
            note,
        ]
    );
    assert_eq!(
        said(&runtime, "show fixture"),
        [
            format!(
                "* fixture\n  status     ready, 1 tool\n  version    2025-06-18\n  transport  stdio\n  command    /bin/sh {}\n  source     ~/.xfx/mcp.json\n  tools      1\n    echo  Echo text back through the test fixture",
                fixture()
            ),
            note.to_string(),
        ]
    );
    assert_eq!(
        said(&runtime, "show proj"),
        [
            "* proj\n  status     waiting for approval\n  transport  stdio\n  command    /bin/true 'a b'\n  source     .mcp.json\n  next       /mcp approve proj",
            note,
        ]
    );
    assert_eq!(
        said(&runtime, "show remote"),
        [
            "* remote\n  status     disabled\n  transport  http\n  url        http://127.0.0.1:9/mcp\n  source     ~/.xfx/mcp.json",
            note,
        ]
    );
    for (line, answer) in [
        (
            "login fixture",
            "! fixture runs a command, so it has no login.",
        ),
        (
            "logout fixture",
            "! fixture runs a command, so it has no login.",
        ),
        ("login proj", "! proj runs a command, so it has no login."),
        ("login remote", "! remote is disabled."),
        ("logout remote", "* remote wasn't signed in."),
    ] {
        assert_eq!(said(&runtime, line), [answer], "{line}");
    }
    runtime.shutdown();
}

#[test]
fn a_remote_server_is_refused_a_login_by_fxs_checks_then_by_the_gate() {
    let scratch = Scratch::new();
    scratch.write_profile(
        r#"{"mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:9/mcp"},"old":{"type":"sse","url":"http://127.0.0.1:9/sse"}}}"#,
    );
    std::fs::write(
        scratch.project(),
        r#"{"mcpServers":{"web":{"type":"http","url":"http://127.0.0.1:9/web"}}}"#,
    )
    .unwrap();
    let runtime = scratch.runtime();
    for (line, answer) in [
        (
            "login web",
            "! web is waiting for approval: /mcp approve web.",
        ),
        ("login old", "! old is not supported."),
        (
            "login --no-browser remote",
            "! Interactive MCP authentication is unavailable here.",
        ),
        ("logout remote", "* remote wasn't signed in."),
        ("logout web", "* web wasn't signed in."),
    ] {
        assert_eq!(said(&runtime, line), [answer], "{line}");
    }
    runtime.shutdown();
}

#[test]
fn approve_and_reject_keep_the_choice_and_say_what_it_runs() {
    let scratch = Scratch::new();
    scratch.write_profile("{}");
    std::fs::write(
        scratch.project(),
        r#"{"mcpServers":{"proj":{"command":"/bin/true","args":["it's"]},"web":{"type":"http","url":"http://127.0.0.1:9/web"}}}"#,
    )
    .unwrap();
    let runtime = scratch.runtime();
    assert_eq!(
        said(&runtime, "approve proj"),
        ["* Approved proj, which runs: /bin/true 'it'\\''s'"]
    );
    let settings = std::fs::read_to_string(scratch.settings()).unwrap();
    assert!(settings.contains("\"enabledMcpjsonServers\""), "{settings}");
    assert!(settings.contains("\"proj\""), "{settings}");
    assert_eq!(
        said(&runtime, "reject proj"),
        ["* Rejected proj. xfx won't start it in this project."]
    );
    let settings = std::fs::read_to_string(scratch.settings()).unwrap();
    assert!(
        settings.contains("\"disabledMcpjsonServers\""),
        "{settings}"
    );
    // `--all`: every project server not rejected, as fx lists them.
    assert_eq!(
        said(&runtime, "approve --all"),
        ["* Approved web, which connects to: http://127.0.0.1:9/web"]
    );
    assert_eq!(
        said(&runtime, "reject fixture"),
        ["! No project server named fixture in .mcp.json."]
    );
    runtime.shutdown();
}

#[test]
fn the_completions_are_the_verbs_then_each_servers_lines_with_its_status() {
    let scratch = Scratch::new();
    scratch.write_profile(
        r#"{"mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:9/mcp","enabled":false},"bad.name":{"command":"/bin/true","enabled":false}}}"#,
    );
    std::fs::write(
        scratch.project(),
        r#"{"mcpServers":{"proj":{"command":"/bin/true"}}}"#,
    )
    .unwrap();
    let runtime = scratch.runtime();
    let lines: Vec<String> = completions(&runtime)
        .into_iter()
        .map(|(full, description)| format!("{full} | {description}"))
        .collect();
    assert_eq!(
        lines,
        [
            "/mcp list | configured servers",
            "/mcp show | connect to a server and list its tools",
            "/mcp add | add a server",
            "/mcp remove | remove a server and its login",
            "/mcp login | sign in to an HTTP server",
            "/mcp logout | sign out of an HTTP server",
            "/mcp approve | approve a project server",
            "/mcp reject | reject a project server",
            "/mcp show remote | disabled",
            "/mcp remove remote | disabled",
            "/mcp login remote | disabled",
            "/mcp logout remote | disabled",
            "/mcp show proj | waiting for approval",
            "/mcp remove proj | waiting for approval",
            "/mcp approve proj | waiting for approval",
            "/mcp reject proj | waiting for approval",
        ]
    );
    runtime.shutdown();
}

#[test]
fn a_name_or_a_target_reaches_the_screen_spelled() {
    assert_eq!(safe("a\u{1b}[2Jb"), "a\\x1b[2Jb");
    assert_eq!(safe("  two\n\tlines  "), "two lines");
    assert_eq!(safe("rtl\u{202e}x"), "rtl\\u{202e}x");
    let long = "x".repeat(300);
    let cut = safe(&long);
    assert_eq!(cut.len(), 240);
    assert!(cut.ends_with("..."));
    // Cut at a whole token: never inside an escape.
    let cut = safe(&"\u{1b}".repeat(100));
    assert!(cut.ends_with("\\x1b..."), "{cut}");
    assert!(cut.len() <= 240);
    assert_eq!(
        command_line("node", &["server.js".into(), "--port".into(), "3".into()]),
        "node server.js --port 3"
    );
    assert_eq!(
        command_line(
            "npx",
            &["".into(), "a b".into(), "it's".into(), "$HOME".into()]
        ),
        "npx '' 'a b' 'it'\\''s' '$HOME'"
    );
}
