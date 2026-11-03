//! The interactive shell: what a bare `xfx` runs.
//!
//! It is deliberately not a full-screen TUI. It never takes the alternate
//! screen, never puts the terminal in raw mode, and never repaints anything it
//! has already written, so the transcript above it stays exactly where the user
//! left it and every line xfx prints is scrollback afterwards. Line editing is
//! the terminal's own: the kernel's canonical mode already provides backspace,
//! word erase, and line kill, and a shell that took those over would have to
//! restore a terminal it had changed. This one has nothing to restore
//! (`vercel-labs/fx@580a0c5d src/core/app/app_entry_runtime.zig:224` is the
//! upstream refusal to run without a terminal at all).
//!
//! Everything below the prompt is the same product as the command line: one
//! [`crate::agent::TurnMachine`] per prompt, a bundle from the same
//! [`crate::provider::Bundle::select`], the same [`crate::tools`] registry under the
//! same [`crate::permission`] authority, and the same
//! [`crate::session::SessionStore`]. The shell adds a loop, the slash commands
//! of [`SLASH_REGISTRY`], and an interrupt policy -- not a second way to talk to a model, and not a
//! second place that decides which backend a prompt goes to.

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use crate::agent::{run_turn_with_skills, TurnRequest};
use crate::app::{spawn_interrupt_thread, AppError, INTERRUPT_NOTICE};
use crate::clipboard::{last_reply, Clipboard, COPIED, COPY_FAILED, NO_REPLY};
use crate::config::{Environment, PermissionMode, RuntimeConfig, StatusLineItem};
use crate::gateway::{CancelToken, Provider, DEFAULT_MAX_ATTEMPTS};
use crate::output::{
    safe_one_line, Event, EventSink, OutputFormat, SessionFacts, StatusSnapshot, TextSink,
    SANDBOX_LABEL,
};
use crate::permission::{LiveRules, PermissionSession, YOLO_WARNING};
use crate::provider::model::{ModelOutcome, ModelRequest, ModelSelector};
use crate::provider::{Bundle, ProviderId};
use crate::session::{
    validate_title, NewSession, SessionEvent, SessionId, SessionRecorder, SessionStore, TitleError,
    WritableSession,
};
use crate::tools::{QuestionRequester, ToolContext};
use crate::workspace::access::{self as workspace_access, WorkspaceAccess};
use crate::workspace::{AccessScope, ProjectContext};

/// Every slash command the shell accepts, in the order `/help` lists them.
///
/// The set is closed for the same reason the command grammar is: a name printed
/// by `/help` is a promise. `scripts/check-no-stubs.sh` reads this declaration
/// textually and requires an `implemented` row in `docs/parity.md` for each
/// entry, and refuses any name a `deferred` row claims.
///
/// It stays a flat list of **canonical names** even though [`SLASH_REGISTRY`]
/// below carries the same names with everything else known about them. The two
/// are pinned to each other at compile time, so the duplication cannot drift,
/// and the reason for it is the honesty gate: that script must be able to read
/// the advertised set out of the source *without building it*, so a repository
/// whose build is broken still cannot hide a broken promise.
///
/// The layout is pinned with `rustfmt::skip` because that script matches the
/// opening line; see [`crate::cli::ADVERTISED_COMMANDS`].
#[rustfmt::skip]
pub const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/clear",
    "/new",
    "/reset",
    "/resume",
    "/rename",
    "/provider",
    "/stats",
    "/usage",
    "/status",
    "/image",
    "/images",
    "/model",
    "/permissions",
    "/allowlist",
    "/undo",
    "/mcp",
    "/skills",
    "/copy",
    "/trace",
    "/compact",
    "/settings",
    "/alias",
    "/credits",
    "/paste",
    "/fast",
    "/ultrafast",
    "/statusline",
    "/sound",
    "/workspace",
    "/version",
    "/quit",
];

/// One slash command, whole.
///
/// What a front end needs to know about a command in one place: what it is
/// called, what else it answers to, what `/help` says about it, and whether the
/// rest of the line is an argument or noise. Before this existed each of those
/// lived in its own `match` -- and a `match` per fact is a fact per place it can
/// be got wrong, which is exactly how a completion menu ends up offering a name
/// the parser does not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashSpec {
    /// The command this describes.
    pub command: Slash,
    /// The one name `/help` lists it under, leading slash included.
    pub name: &'static str,
    /// Every other name that reaches the same command.
    ///
    /// An alias is **not** a command of its own: it is metadata on this row, it
    /// is never printed as a name of its own, and [`SLASH_COMMANDS`] does not
    /// grow by one when it is added. What it does is make the parser answer it and
    /// the completion menu offer the command it belongs to.
    pub aliases: &'static [&'static str],
    /// The single line `/help` prints beside the name, and the menu beside it.
    pub summary: &'static str,
    /// The group the completion menu files it under, at the right of its row,
    /// and the tab the TUI's `/help` catalog lists it on.
    ///
    /// One of [`SLASH_CATEGORIES`], upstream's own names
    /// (`SlashPresentationCategory.label`,
    /// `vercel-labs/fx@c1db9191 src/core/slash_commands/command_specs.zig:143-157`).
    /// A string rather than an `enum` because nothing branches on *which* one it
    /// is: the menu shows it and the catalog groups by it, and the assertion
    /// under the registry keeps it to that closed set at compile time.
    pub category: &'static str,
    /// Whether the rest of the line means anything to it.
    ///
    /// The completion menu's reason for existing: completing a command that
    /// takes an argument leaves the caret past a space, and completing one that
    /// does not leaves it at the end of the name.
    pub has_args: bool,
}

/// Every canonical command, in the order `/help` lists them, with everything
/// known about each.
///
/// **One declaration, three readers**: the parser ([`Slash::parse`]), the help
/// page ([`help_text`]), and the TUI's completion menu (`crate::tui`'s
/// `picker`). A name, a summary or an alias that only some of them agreed about
/// would be a command surface that answers differently depending on where you
/// asked -- which is the one thing a command surface must never be.
///
/// **Upstream's order, upstream's categories, and upstream's words where the
/// command is upstream's command** (`vercel-labs/fx@c1db9191
/// src/builtins/commands.zig:463-499`), because the menu shows all three. The
/// rows xfx does not have are absent rather than stubbed. One summary stays
/// xfx's own, because upstream's sentence describes something this command
/// does not do: `/model` (upstream chooses a reasoning effort as well,
/// `:478`; xfx sends none). `/clear`, `/new` and `/reset` are one operation, as
/// upstream's are (`app_commands.zig:667-680`), and carry upstream's three
/// sentences (`:464-466`). `/provider` is upstream's row whole -- name, `/setup`
/// alias, sentence and category (`:472`) -- and is the one whose sentence
/// promises more than xfx's picker shows: the provider column is there, the
/// sign-in columns after it are gated (zbrain open-loop #88) and absent, as
/// `docs/parity.md` says.
/// `/permissions` is upstream's sentence with this product's name in it
/// (`:479`). `/stats` keeps upstream's sentence although what it prints is the
/// terminal writer's counters rather than tokens: so does upstream's
/// (`commandShowStats`, `app_commands.zig:1009-1022`), and the description is
/// the one the menu shows beside the name.
#[rustfmt::skip]
pub const SLASH_REGISTRY: &[SlashSpec] = &[
    SlashSpec {
        command: Slash::Help,
        name: "/help",
        aliases: &[],
        summary: "show available slash commands",
        category: "General",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Clear,
        name: "/clear",
        aliases: &[],
        summary: "start a fresh conversation while keeping managed processes",
        category: "General",
        has_args: false,
    },
    SlashSpec {
        command: Slash::New,
        name: "/new",
        aliases: &[],
        summary: "start a fresh session",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Reset,
        name: "/reset",
        aliases: &[],
        summary: "reset the current session context",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Resume,
        name: "/resume",
        aliases: &[],
        summary: "resume a saved session",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Rename,
        name: "/rename",
        aliases: &[],
        summary: "rename the current session",
        category: "Session",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Setup,
        name: "/provider",
        // Upstream's own alias for the same command
        // (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:472`), and the
        // name this command had before it was upstream's row.
        aliases: &["/setup"],
        summary: "choose the model provider and how it signs in",
        category: "Account",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Stats,
        name: "/stats",
        aliases: &[],
        summary: "show token and turn statistics",
        category: "Account",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Usage,
        name: "/usage",
        // Upstream's own alias for the same command
        // (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:474`).
        aliases: &["/cost"],
        // Upstream's sentence with this product's name in it (`:474`): what
        // it shows is what xfx itself spent.
        summary: "show local xfx tokens, models, and spend",
        category: "Account",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Status,
        name: "/status",
        aliases: &[],
        summary: "show runtime configuration",
        category: "General",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Image,
        name: "/image",
        // Upstream's row (`commands.zig:476`): its alias, its sentence, and
        // the category its tab is.
        aliases: &["/img"],
        summary: "attach an image by path",
        category: "Media",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Images,
        name: "/images",
        aliases: &[],
        summary: "manage pending image attachments",
        category: "Media",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Model,
        name: "/model",
        aliases: &[],
        summary: "show the model, or `/model <id>` to use another one from now on",
        category: "Model",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Permissions,
        name: "/permissions",
        aliases: &[],
        summary: "choose what xfx is allowed to do",
        category: "Security",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Allowlist,
        name: "/allowlist",
        aliases: &[],
        // Upstream's sentence (`commands.zig:480`). URLs included: xfx keeps
        // and lists `url` and `web-fetch-domain` rules as upstream does,
        // although no xfx tool reaches a URL for one to decide.
        summary: "manage trusted commands, tools, and URLs",
        category: "Security",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Undo,
        name: "/undo",
        aliases: &[],
        // Upstream's row whole (`commands.zig:481`): a Session command that
        // takes nothing.
        summary: "undo the latest tracked file operation",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Mcp,
        name: "/mcp",
        aliases: &[],
        // Upstream's row whole (`commands.zig:482`): its words, its place
        // after `/undo`, and the Extensions tab.
        summary: "manage local and remote MCP servers, resources, prompts, and project trust",
        category: "Extensions",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Skills,
        name: "/skills",
        aliases: &[],
        // Upstream's row (`commands.zig:483`): its sentence, its category, and
        // an argument. The grammar is `crate::skills::command::parse`, whose
        // refusal is upstream's usage line.
        summary: "browse and manage skills",
        category: "Extensions",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Copy,
        name: "/copy",
        aliases: &[],
        summary: "copy the last assistant response",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Trace,
        name: "/trace",
        aliases: &[],
        // Upstream's row whole (`commands.zig:486`): its words, its category,
        // and no argument.
        summary: "copy a private diagnostic trace",
        category: "Product",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Compact,
        name: "/compact",
        aliases: &[],
        // Upstream's row whole (`commands.zig:487`).
        summary: "summarize context into a fresh window",
        category: "Session",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Settings,
        name: "/settings",
        aliases: &[],
        // Upstream's words (`commands.zig:488`). It takes an argument there
        // (`startup-scrollback [on|off]`) and here, so completing it leaves the
        // caret past a space as upstream's popup does.
        summary: "browse and update settings",
        category: "Appearance",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Alias,
        name: "/alias",
        aliases: &[],
        // Upstream's row whole (`commands.zig:489`): its words, its place
        // between `/settings` and `/credits`, and the Extensions tab. It takes
        // the rest of the line there (`/alias [name] [command]` is its help
        // entry) and the answer is the same sentence whatever that is
        // (`commandHandleAlias`, `app_commands.zig:2043-2050`).
        summary: "show alias availability",
        category: "Extensions",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Credits,
        name: "/credits",
        // Upstream's own alias for the same command
        // (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:490`).
        aliases: &["/balance"],
        summary: "show gateway credits balance",
        category: "Account",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Paste,
        name: "/paste",
        aliases: &[],
        // Upstream's sentence (`commands.zig:491`); it takes nothing.
        summary: "attach an image from the clipboard when supported",
        category: "Media",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Fast,
        name: "/fast",
        aliases: &[],
        // Upstream's words and place (`commands.zig:492`): a toggle that takes
        // nothing, so `/fast on` is no command at all.
        summary: "toggle Fast mode when supported",
        category: "Model",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Ultrafast,
        name: "/ultrafast",
        aliases: &[],
        // Upstream's words (`commands.zig:493`); the grammar is
        // `/ultrafast [on|off|status]`, which its refusal prints.
        summary: "request Ultra mode when supported",
        category: "Model",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Statusline,
        name: "/statusline",
        aliases: &[],
        // Upstream's words (`commands.zig:494`); the grammar is
        // `STATUSLINE_USAGE`, which the refusal prints.
        summary: "toggle status line segments",
        category: "Appearance",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Sound,
        name: "/sound",
        aliases: &[],
        // Upstream's sentence (`commands.zig:495`). The bell is the one sound
        // xfx makes, and it is a sound: what the terminal does with it -- a
        // beep, a flash, a mark on a multiplexer's tab -- is the terminal's.
        summary: "toggle sounds and terminal bells",
        category: "Appearance",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Workspace,
        name: "/workspace",
        aliases: &[],
        // Upstream's row whole (`commands.zig:496`); the grammar is
        // `crate::workspace::access::parse`, whose refusal is upstream's
        // usage line.
        summary: "manage additional workspace directories",
        category: "Workspace",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Version,
        name: "/version",
        aliases: &[],
        summary: "show the xfx version",
        category: "General",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Quit,
        name: "/quit",
        // Upstream's own alias for the same command
        // (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:499`), and the
        // only one xfx answers.
        aliases: &["/exit"],
        summary: "exit the interactive shell",
        category: "General",
        has_args: false,
    },
];

/// Every category a command can be filed under, in the order the `/help`
/// catalog's tabs are in.
///
/// Upstream's `SlashPresentationCategory`, in its declaration order
/// (`vercel-labs/fx@c1db9191 src/core/slash_commands/command_specs.zig:130-157`),
/// which is also the order the catalog lists commands in (`helpCatalogSpecAt`,
/// `:429-440`). **All eleven, including the ones no xfx command is filed
/// under**: a tab is a category, not a command, and upstream's own catalog has
/// a tab with nothing on it (`Agents` at `c1db919`), so an empty one says
/// "nothing here" rather than advertising anything.
pub const SLASH_CATEGORIES: &[&str] = &[
    "General",
    "Session",
    "Account",
    "Model",
    "Appearance",
    "Security",
    "Workspace",
    "Media",
    "Agents",
    "Extensions",
    "Product",
];

/// Every registry row is filed under one of [`SLASH_CATEGORIES`], or this crate
/// does not build: a row under a name the catalog has no tab for would be a
/// command the `/help` catalog never lists.
const _: () = {
    let mut row = 0;
    while row < SLASH_REGISTRY.len() {
        let mut found = false;
        let mut category = 0;
        while category < SLASH_CATEGORIES.len() {
            if same(SLASH_REGISTRY[row].category, SLASH_CATEGORIES[category]) {
                found = true;
            }
            category += 1;
        }
        assert!(found);
        row += 1;
    }
};

/// The registry row for one command.
///
/// Total by construction: the assertion below this proves at **compile time**
/// that the registry names the same commands as [`SLASH_COMMANDS`], in the
/// same order, so the scan cannot come back empty for a `Slash` that exists.
pub fn slash_spec(command: Slash) -> &'static SlashSpec {
    SLASH_REGISTRY
        .iter()
        .find(|spec| spec.command == command)
        .expect("every command has exactly one registry row")
}

/// The two declarations agree, or this crate does not build.
///
/// A unit test would find the same drift a moment later; a `const` assertion
/// finds it without running anything, which matters because one of the two
/// lists is read by a shell script that never builds the crate at all. What it
/// cannot check is that each row's `command` is the one its name belongs to --
/// `Slash` has no `const` equality -- and `the_canonical_list_and_the_registry_are_one_order`
/// is what checks that half.
const _: () = {
    assert!(SLASH_REGISTRY.len() == SLASH_COMMANDS.len());
    let mut index = 0;
    while index < SLASH_REGISTRY.len() {
        assert!(same(SLASH_REGISTRY[index].name, SLASH_COMMANDS[index]));
        index += 1;
    }
};

/// Whether two names are the same, in a `const` context.
///
/// `str`'s own comparison is not usable in one, and the alternative to eight
/// lines here is not checking the agreement until something runs.
const fn same(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// What the shell prints before reading a line.
pub const PROMPT: &str = "> ";

/// What `/stats` says on this surface.
///
/// The counters are the terminal writer's -- bytes delivered, whole repaints,
/// coalesced resizes (`crate::tui`'s `shell::Metrics`) -- and this shell has none of
/// the three: it prints lines and lets the terminal scroll them. A row of
/// zeros here would be a report about frames that were never drawn, so it
/// says where the numbers are instead.
pub const STATS_ON_THE_LINE_SHELL: &str = "xfx: /stats counts the frames the full-screen shell \
     draws, and this line-oriented shell draws none; without XFX_TUI=0, xfx opens the full-screen one";

/// What `/compact` says on this surface.
///
/// Upstream's compaction is a piece of the full-screen shell's work: it
/// shows on the band's activity row and nowhere else, waits behind a running
/// turn, and is stopped by the keys that stop one
/// (`compactionProjection`, `activity_status.zig:16-41`). This shell has no
/// row to show it on, so it says where it is rather than compacting a
/// conversation the user cannot watch being compacted.
pub const COMPACT_ON_THE_LINE_SHELL: &str = "xfx: /compact runs in the full-screen shell, which \
     shows its progress; without XFX_TUI=0, xfx opens the full-screen one";

/// What `/usage` says on this surface.
///
/// Upstream's `/usage` is a dashboard in place of its footer
/// (`commandShowUsage`, `app_commands.zig:1024-1062`), with keys that walk
/// windows and models, and this shell reads a line at a time and has no
/// footer. It says where the dashboard is, as `/stats` says where the
/// counters are, rather than printing a different report under the same name.
pub const USAGE_ON_THE_LINE_SHELL: &str = "xfx: /usage shows its dashboard in the full-screen \
     shell, and this line-oriented shell has no footer to show it in; without XFX_TUI=0, xfx \
     opens the full-screen one";

/// What `/resume` says on this surface.
///
/// Upstream's `/resume` is a picker in its footer
/// (`commandResumeSession`, `app_commands.zig:682-693`), and this shell has no
/// footer to put one in: it reads a line at a time. It says where the picker
/// is, as `/stats` says where the counters are, rather than resuming something
/// the user did not pick.
pub const RESUME_ON_THE_LINE_SHELL: &str = "xfx: /resume picks a saved session in the full-screen \
     shell, and this line-oriented shell has no picker; without XFX_TUI=0, xfx opens the \
     full-screen one, and `xfx --resume <last|ID>` opens it on a session";

/// What `/fast` and `/ultrafast` say on this surface.
///
/// They choose the speed a turn asks the provider for, and only the
/// full-screen shell asks for one: this shell sends neither a reasoning effort
/// nor a speed with its requests. It says where the switches are, as `/stats`
/// says where the counters are, rather than accepting a switch that would
/// change nothing.
pub const SPEED_ON_THE_LINE_SHELL: &str = "xfx: /fast and /ultrafast set the speed the \
     full-screen shell asks for, and this line-oriented shell asks for none; without \
     XFX_TUI=0, xfx opens the full-screen one";

/// What `/image`, `/images` and `/paste` say on this surface.
///
/// An attachment is a badge in the prompt being written, and this shell reads
/// a finished line at a time: there is no draft for an image to wait in, and
/// a path typed here is sent to the model as the words it is. It says where the
/// attachments are, as `/stats` and `/resume` say where theirs are.
pub const IMAGES_ON_THE_LINE_SHELL: &str = "xfx: /image, /images and /paste attach images to the \
     prompt being written in the full-screen shell, and this line-oriented shell has no draft to \
     attach them to; without XFX_TUI=0, xfx opens the full-screen one";

/// What `/rename` says when there is no title to give the session, or one it
/// cannot have (`handleRenameCommand`, `app_commands.zig:3815-3858`): the
/// words both shells use.
pub(crate) fn title_refusal(problem: TitleError) -> &'static str {
    match problem {
        TitleError::Empty => "usage: /rename <title>",
        TitleError::TooLong => "title is too long",
        TitleError::NotPrintable => "title must be printable text",
    }
}

/// Gives `conversation`'s session the title `title`, already validated
/// ([`crate::session::validate_title`]), and says whether it was written down.
///
/// The recorder's first failure is the answer when there is one, this write's
/// or an earlier one's: a recorder that has failed writes nothing more
/// (`SessionRecorder::commit`), so a title it was handed afterwards is a title
/// the log does not have.
pub(crate) fn rename_conversation(
    conversation: &mut Conversation,
    title: &str,
) -> Result<(), String> {
    conversation.recorder.commit(SessionEvent::SessionRenamed {
        title: title.to_string(),
        source: crate::session::TitleSource::User,
    });
    match conversation.recorder.failure() {
        Some(problem) => Err(problem.to_string()),
        None => Ok(()),
    }
}

/// What `/settings` says to an argument it does not take, upstream's words
/// exactly (`writeSettingsUsage`, `session_commands.zig:1129-1135`).
pub const SETTINGS_USAGE: &str = "usage: /settings [startup-scrollback [on|off]]";

/// What `/settings` says on this surface.
///
/// Upstream's `/settings` is a menu in its footer (`commandHandleSettings`,
/// `app_commands.zig:2022-2041`), and this shell has no footer to put one in.
/// It says where the menu is, as `/resume` says where its picker is.
pub const SETTINGS_ON_THE_LINE_SHELL: &str = "xfx: /settings browses and changes settings in \
     the full-screen shell, and this line-oriented shell has no menu; without XFX_TUI=0, xfx \
     opens the full-screen one";

/// `/mcp` on the line shell. The MCP client runs in the full-screen shell's
/// session -- its servers, its menu, its trust prompt -- and this shell starts
/// no server, so it says where they are rather than managing a profile it
/// would never connect to.
pub const MCP_ON_THE_LINE_SHELL: &str = "xfx: /mcp manages MCP servers in the full-screen \
     shell, which starts and connects them; this line-oriented shell runs none. Without \
     XFX_TUI=0, xfx opens the full-screen one";

/// What `/alias` is filed under, on both surfaces: upstream's topic, which is
/// the plural (`commandHandleAlias`, `app_commands.zig:2046`).
pub const ALIAS_TOPIC: &str = "aliases";

/// What `/alias` says, bare or with anything after it: upstream's sentence,
/// a neutral notice (`app_commands.zig:2047-2048`). Upstream answers every form
/// of `/alias [name] [command]` with it and keeps nothing, and so does xfx --
/// there is no alias table on either side for a name to go into.
pub const ALIASES_NOT_CONFIGURABLE: &str = "Aliases are not yet configurable.";

/// What every `/statusline` notice is filed under, on both surfaces.
pub const STATUSLINE_TOPIC: &str = "statusline";

/// `/statusline`'s refusal, upstream's words exactly
/// (`handleStatuslineCommand`, `app_commands.zig:3931-3941`).
pub const STATUSLINE_USAGE: &str = "usage: /statusline [context|session|workspace]";

/// What a `/statusline` write that landed says (`reportUserSettingsCommit`,
/// `session_commands.zig:27-84`): the profile is the one scope xfx writes.
pub const STATUSLINE_SAVED: &str = "saved to user settings (scope=user)";

/// What `/statusline` says an item is now: `context: on`
/// (`applyStatuslineItem`, `app_commands.zig:3915-3923`).
pub fn statusline_state(item: StatusLineItem, on: bool) -> String {
    format!("{}: {}", item.name(), if on { "on" } else { "off" })
}

/// Keeps one `/statusline` item in the profile at `path`, or says why it could
/// not.
///
/// One preference, through the profile's one writer
/// ([`crate::provider::profile::write_preference`]): read, set
/// `statusLine.<item>`, write back staged and owner-only. A session with no
/// home has no profile to keep it in, and that is the reason it says.
pub fn save_statusline(
    path: Option<&std::path::Path>,
    item: StatusLineItem,
    on: bool,
) -> Result<(), String> {
    let Some(path) = path else {
        return Err("no home directory is set".to_string());
    };
    crate::provider::profile::write_preference(path, &item.key_path(), serde_json::Value::Bool(on))
        .map_err(|err| err.to_string())
}

/// What a `/statusline` write that failed says: the change is in force for
/// this process and nowhere else (`reportUserSettingsFailure`,
/// `session_commands.zig:110-133`, whose `runtime_changed` is always true
/// here -- every `/statusline` change is a flip).
pub fn statusline_not_saved(reason: &str) -> String {
    format!("active for this process but not saved to user settings ({reason})")
}

/// Why xfx will not open a shell here.
pub const NO_TERMINAL: &str = "xfx requires an interactive terminal (TTY); \
     run `xfx ask <prompt>` when there is not one";

/// Why xfx will not open a shell it could not record.
pub const NO_STORE: &str = "xfx cannot record a conversation because no home directory is set; \
     run `xfx ask --no-save <prompt>` to ask without recording";

/// What xfx says on the interrupt that ends it.
pub const LEAVING_NOTICE: &str = "xfx: interrupted -- leaving.";

/// The exit status of a process that stopped because it was interrupted.
const INTERRUPTED_EXIT_CODE: i32 = 130;

/// The most bytes of a mistyped command xfx quotes back.
///
/// A slash command is a word, and a word that runs past this is not a typo the
/// user needs to see in full -- it is something pasted, and the part of the
/// refusal worth reading is the guidance at the end of the line.
const MAX_QUOTED_COMMAND_BYTES: usize = 60;

// ---------------------------------------------------------------------------
// what a submitted line is
// ---------------------------------------------------------------------------

/// One canonical slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slash {
    Help,
    New,
    Clear,
    /// The same operation as [`Slash::New`] and [`Slash::Clear`] under
    /// upstream's third name for it (`commandResetSession`,
    /// `app_commands.zig:672-675`): the policy upstream passes it is thrown
    /// away before anything reads it (`app_session_runtime.zig:2001`), so it
    /// opens a new session exactly as the other two do.
    Reset,
    /// Pick a saved session and continue it: the TUI's session picker
    /// (`commandResumeSession`, `app_commands.zig:682-693`).
    Resume,
    /// Give the session a title of the user's own, which outranks the one its
    /// first prompt would give it (`handleRenameCommand`,
    /// `app_commands.zig:3815-3858`).
    Rename,
    Model,
    /// Switch the provider a turn talks to, and record the choice.
    ///
    /// The only command in the palette that writes to `~/.xfx/settings.json`
    /// and the only one that reaches the network, which is why it is a command
    /// of its own rather than an argument of `/model`: `/model` chooses among
    /// what the *current* provider offers, and this chooses the provider.
    ///
    /// `/provider`, with `/setup` as its alias. The variant keeps the name the
    /// command had when `/setup` was canonical: it names what the command does
    /// (`provider::setup`), and every surface reaches it by parsing the
    /// registry rather than by spelling it.
    Setup,
    /// What the session may do without asking, and the way to change it:
    /// `ask`, `auto`, `full-access` or `reset`
    /// ([`crate::permission::slash`]).
    Permissions,
    /// The persistent allow rules: view them, or add, remove and reset them
    /// in the profile's table or this workspace's
    /// ([`crate::permission::allowlist`]).
    Allowlist,
    /// Put the file the conversation's tools changed last back as it was
    /// before ([`crate::tools::undo`]).
    Undo,
    /// The session's MCP servers: upstream's menu, its text subcommands and
    /// its project trust (`commandHandleMcp`, `app_commands.zig:1312-1405`;
    /// [`crate::mcp`]).
    Mcp,
    /// The skills the session can see: a menu in the TUI, `show`, `path`,
    /// `create`, `remove` and `add` from a local directory on either surface
    /// ([`crate::skills::command`]).
    Skills,
    /// Put the last thing the model said on the system clipboard
    /// ([`crate::clipboard`]).
    Copy,
    /// Fast mode on or off, for a model that offers it
    /// (`toggleFastForModel`, `session_commands.zig:733-751`).
    Fast,
    /// Ultra mode on, off, or asked about, for a model that offers it
    /// (`handleUltrafast`, `session_commands.zig:704-723`).
    Ultrafast,
    /// Attach the image a path names to the prompt being written
    /// ([`crate::image`]).
    Image,
    /// What the prompt being written has attached, or `clear` to drop it all.
    Images,
    /// Attach the image on the clipboard ([`crate::clipboard::Clipboard::read_image`]).
    Paste,
    /// Write a private diagnostic report about this session and put the file
    /// on the clipboard ([`crate::trace`]).
    Trace,
    /// Replace the older turns of the conversation with a compacted form the
    /// model reads in their place (`commandCompactHistory`,
    /// `app_commands.zig:2010-2020`; [`crate::agent::compact`]).
    Compact,
    /// Upstream's sound switch, which here is the terminal bell
    /// ([`crate::sound`]).
    Sound,
    /// What the terminal writer has done this session: upstream's five render
    /// counters.
    Stats,
    /// What xfx itself has spent -- tokens, requests and the provider's own
    /// cost, by model -- over the last 30 days, 7 days, 24 hours or this
    /// session ([`crate::usage`]).
    Usage,
    /// What the Gateway account has left ([`crate::credits`]).
    Credits,
    /// The configuration a turn would run under, and the live conversation's
    /// own two counts.
    Status,
    /// Which of the footer's optional segments are shown: a picker in the
    /// TUI, one item flipped by name on either surface, and the choice kept in
    /// the profile (`statusLine.{context,session,workspace}`).
    Statusline,
    /// Every preference the session can change, by category, in one menu
    /// that changes each in place: the TUI's settings menu
    /// (`commandHandleSettings`, `app_commands.zig:2022-2041`).
    Settings,
    /// Upstream's `/alias [name] [command]`, which says aliases cannot be
    /// configured yet and keeps nothing (`commandHandleAlias`,
    /// `app_commands.zig:2043-2050`).
    Alias,
    /// The directories a session may work in besides the workspace, saved in
    /// the profile: a picker in the TUI, `list`, `add`, `remove` and `clear`
    /// on either surface ([`crate::workspace::access`]).
    Workspace,
    Version,
    Quit,
}

impl Slash {
    /// The name as typed.
    ///
    /// Through the registry, like everything else that is known about a
    /// command: this used to be a `match` of its own, and a `match` that agreed
    /// with `/help`'s list only because someone kept them in step. There is no
    /// `summary` beside it any more for the same reason -- `/help` reads
    /// [`SLASH_REGISTRY`] directly, so there is nothing for a second accessor
    /// to disagree with.
    pub fn name(self) -> &'static str {
        slash_spec(self).name
    }

    /// The command `token` names, if it names one.
    ///
    /// **The registry is the whole grammar**, canonical names and aliases
    /// together, so a front end cannot answer a name the help page does not
    /// know about and cannot refuse one it does. The match stays exact and
    /// case-sensitive for the reason [`classify`] gives.
    fn parse(token: &str) -> Option<Self> {
        SLASH_REGISTRY
            .iter()
            .find(|spec| spec.name == token || spec.aliases.contains(&token))
            .map(|spec| spec.command)
    }
}

/// What one line the user submitted turns out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submitted {
    /// Nothing was typed. Nothing happens.
    Blank,
    /// One canonical command, with the rest of the line as its argument.
    Command { command: Slash, argument: String },
    /// A line that begins with `/` and names nothing.
    UnknownCommand { token: String },
    /// A prompt for the model.
    Prompt(String),
}

/// Decides what a submitted line is.
///
/// A slash command is a line whose *first* character is `/`, so `what does
/// a/b mean` is a prompt and `/nonesuch` is a mistake rather than a question.
/// The match is exact and case-sensitive: guessing at `/HELP` or `/hep` would
/// make the refusal nondeterministic, which is the one thing a command surface
/// must never be.
pub fn classify(line: &str) -> Submitted {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Submitted::Blank;
    }
    if !trimmed.starts_with('/') {
        return Submitted::Prompt(trimmed.to_string());
    }
    let (token, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((token, rest)) => (token, rest.trim()),
        None => (trimmed, ""),
    };
    match Slash::parse(token) {
        Some(command) => Submitted::Command {
            command,
            argument: rest.to_string(),
        },
        None => Submitted::UnknownCommand {
            token: token.to_string(),
        },
    }
}

/// The exact refusal for a slash command that does not exist.
///
/// The token is quoted back so a user can see their typo, and it is the one
/// thing on this line that xfx did not write. It therefore goes through
/// [`safe_one_line`] first: a line beginning `/` and continuing with an escape
/// sequence would otherwise be echoed straight back and *obeyed* by the
/// terminal -- clearing the screen, retitling the window, moving the cursor --
/// and a very long one would push the guidance off the end of it. The backticks
/// are part of the same job: once control characters have become spaces, the
/// reader still has to be able to see where the quoted text stops.
///
/// **The count is derived, never spelled.** It used to read "the six it has",
/// which was a second declaration of the palette's size beside
/// [`SLASH_COMMANDS`] -- and a second declaration is a second thing that can
/// drift. `/setup` made it wrong the moment it was added, in the one sentence
/// whose whole job is to be exactly true about what the shell has.
pub fn unknown_command_message(token: &str) -> String {
    format!(
        "xfx: `{}` is not an xfx command; /help lists its {} commands",
        safe_one_line(token, MAX_QUOTED_COMMAND_BYTES),
        SLASH_COMMANDS.len()
    )
}

/// The `/help` page.
///
/// **One line per canonical name**, from [`SLASH_REGISTRY`] in its own order,
/// which is what keeps the page and the parser one declaration. An alias is
/// said on the line of the command it belongs to rather than on a line of its
/// own: a page with one line per *name* would be advertising one command per
/// name, and what the shell has is one command answering to two of them.
pub fn help_text() -> String {
    let mut out = String::from("xfx shell commands\n");
    for spec in SLASH_REGISTRY {
        let _ = write!(out, "  {:<9} {}", spec.name, spec.summary);
        if !spec.aliases.is_empty() {
            let _ = write!(out, " (also {})", spec.aliases.join(", "));
        }
        let _ = writeln!(out);
    }
    out.push_str("Anything else is a prompt. Ctrl-C stops a running turn; Ctrl-D leaves.\n");
    out
}

// ---------------------------------------------------------------------------
// interrupts
// ---------------------------------------------------------------------------

/// What the shell is doing when a signal arrives.
#[derive(Debug, Clone, Copy)]
enum Activity {
    /// Waiting for a line. `consecutive` counts interrupts since the last line
    /// the user actually submitted.
    Idle { consecutive: u32 },
    /// Running a turn, and whether it has already been asked to stop.
    Running { cancelled: bool },
}

/// The shell's interrupt policy, shared with the signal thread.
///
/// One lock covers both the activity and the token, so "is this Ctrl-C a
/// cancellation or an exit" and "clear the token for the next turn" cannot
/// interleave. That is the whole reason [`CancelToken::reset`] is safe here.
#[derive(Debug)]
struct Interrupts {
    state: Mutex<Activity>,
    cancel: CancelToken,
}

impl Interrupts {
    fn new(cancel: CancelToken) -> Self {
        Self {
            state: Mutex::new(Activity::Idle { consecutive: 0 }),
            cancel,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Activity> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Marks a turn as running and clears any earlier cancellation.
    fn begin_turn(&self) {
        let mut state = self.lock();
        self.cancel.reset();
        *state = Activity::Running { cancelled: false };
    }

    fn end_turn(&self) {
        *self.lock() = Activity::Idle { consecutive: 0 };
    }

    /// Records that the user submitted a line, so the interrupt count starts
    /// again from there.
    fn line_submitted(&self) {
        let mut state = self.lock();
        if let Activity::Idle { consecutive } = &mut *state {
            *consecutive = 0;
        }
    }

    /// Handles one SIGINT. Runs on the signal thread.
    fn signalled(&self) {
        let mut state = self.lock();
        match *state {
            Activity::Running { cancelled: false } => {
                self.cancel.cancel();
                *state = Activity::Running { cancelled: true };
                let _ = writeln!(io::stderr(), "{INTERRUPT_NOTICE}");
            }
            // Asked twice: the first request is still being honored somewhere
            // and the user has decided not to wait for it.
            Activity::Running { cancelled: true } => std::process::exit(INTERRUPTED_EXIT_CODE),
            Activity::Idle { consecutive } if consecutive >= 1 => {
                let _ = writeln!(io::stderr(), "{LEAVING_NOTICE}");
                std::process::exit(INTERRUPTED_EXIT_CODE)
            }
            Activity::Idle { consecutive } => {
                *state = Activity::Idle {
                    consecutive: consecutive + 1,
                };
                // The line discipline already discarded whatever was typed, so
                // the user is looking at a dead line. Offer a live one.
                let mut stdout = io::stdout();
                let _ = write!(stdout, "\n{PROMPT}");
                let _ = stdout.flush();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the conversation
// ---------------------------------------------------------------------------

/// One session and the tool authority bound to it.
///
/// The two are created and discarded together on purpose. A session grant --
/// the user answering "always" -- is sold as being about *this* session id, and
/// the read proofs that let a file be edited are about what this conversation
/// has seen. `/new` therefore drops both, and `/clear` keeps both.
///
/// Visible to the crate because the TUI's runtime thread opens its conversation
/// through this same call rather than growing a second one of its own
/// (`crate::tui::worker`).
pub(crate) struct Conversation {
    pub(crate) recorder: SessionRecorder,
    pub(crate) tools: ToolContext,
    /// The history notices this conversation has already said
    /// ([`Self::unsaid`]).
    said: std::collections::BTreeSet<String>,
    /// The provider's count of this conversation's last measured request,
    /// which the TUI's compaction scales its estimates by
    /// ([`crate::agent::compact::Calibration`]). Upstream keeps it on the
    /// conversation's agent (`agent.zig:14-19`), so it goes with this one:
    /// `/new` and a resume start without it, and a provider switch keeps it
    /// with the conversation -- a count applies only to the model that made
    /// it ([`crate::agent::compact::Calibration::applies`]), as upstream's
    /// (`orchestrator.zig:7293-7299`).
    pub(crate) calibration:
        std::sync::Arc<std::sync::Mutex<Option<crate::agent::compact::Calibration>>>,
}

impl Conversation {
    /// The lines of `notices` this conversation has not said yet, which are
    /// now said.
    ///
    /// The history is rebuilt for every request, and so are the notices about
    /// what it could not carry -- a reasoning state sealed by another wire, an
    /// image that could not be read back -- so the same line would otherwise
    /// open every turn after a switch or a resume. Said once, it stays true
    /// for the rest of the conversation, as `.prd/04-providers.md`
    /// §Provenance asks ("a one-line user-visible notice"). A new
    /// conversation starts with nothing said.
    pub(crate) fn unsaid(&mut self, notices: Vec<String>) -> Vec<String> {
        notices
            .into_iter()
            .filter(|notice| self.said.insert(notice.clone()))
            .collect()
    }

    /// What `/status` says about this conversation: the turns it holds and the
    /// "always" answers its authority has been given.
    ///
    /// **The live conversation's, not the store's.** `xfx status` reports the
    /// last session on disk because it has no other; a shell has the one it is
    /// talking in, and upstream's report is that one's (`session.historyLen()`
    /// and the live session grants, `session_commands.zig:304-305`). The grants
    /// are the authority's own list rather than the recorder's, because an
    /// answer is held there from the moment it is given and only written down
    /// once its turn is over.
    pub(crate) fn facts(&self) -> SessionFacts {
        SessionFacts {
            history_turns: self.recorder.state().turns.len() as u64,
            permission_grants: self.tools.permissions().grants().len() as u64,
        }
    }

    /// Puts this conversation's authority in `mode`, forgetting every
    /// "always" it holds when `clear_grants` -- `/permissions <mode>` and
    /// `/permissions reset`, on either surface.
    ///
    /// **Here, and not by building a new authority**, because the authority is
    /// built once per conversation and holds what an "always" answer is worth
    /// for the rest of it: a fresh one would forget the grants a mode change
    /// must keep. A mode that differs from the one the log last recorded is
    /// written down, as a resume does (`crate::tui`'s worker, `app::run_ask`),
    /// so a later resume reads what this session really ran under.
    ///
    /// A reset is **durable**. Upstream's `reset` clears the engine's session
    /// grants (`permission_engine.clear`, `app_permission_runtime.zig:144-159`,
    /// `permissions.zig:49-51`) -- the "don't ask again" answers -- and those
    /// are never written down there (`permission_engine.allow` is reached only
    /// from an approval, `app_callbacks.zig:1617-1620`), so a reset session
    /// that is resumed has none. xfx writes the same answers into the log
    /// (`permission_grant_recorded`), so the forgetting is written too
    /// (`permission_grants_cleared`) or a resume would bring them back.
    /// Upstream's saved-session rules -- `remember`, `permissions.json` -- are
    /// a different store, which its `reset` leaves alone and xfx does not
    /// have.
    ///
    /// The order matters: the clearing first, then whatever this session
    /// holds afterwards. An "always" given after a live reset, in the turn
    /// that was running when it happened ([`PermissionSession::clear_grants`]),
    /// may be for a target the log already listed from before the reset, and
    /// `record_new_grants` only writes what the log does not list -- so it is
    /// written once the list has been emptied, or it would be lost with it.
    pub(crate) fn set_permission_mode(&mut self, mode: PermissionMode, clear_grants: bool) {
        let kept = {
            let mut authority = self.tools.permissions();
            authority.set_mode(mode);
            if clear_grants {
                authority.clear_grants();
            }
            authority.grants().to_vec()
        };
        if self.recorder.state().permission_mode != mode {
            self.recorder.commit(SessionEvent::PreferencesChanged {
                model: None,
                permission_mode: Some(mode.label().to_string()),
                provider: None,
                effort: None,
                fast_mode: None,
                ultrafast_mode: None,
            });
        }
        if clear_grants {
            self.recorder.commit(SessionEvent::PermissionGrantsCleared);
            self.recorder.record_new_grants(&kept);
        }
    }

    /// This conversation's grants, one ` - <tool> -> <target>` row each
    /// ([`crate::permission::slash::grant_line`]), not yet made safe to print.
    pub(crate) fn grant_lines(&self, workspace_root: &std::path::Path) -> Vec<String> {
        self.tools
            .permissions()
            .grants()
            .iter()
            .map(|grant| {
                crate::permission::slash::grant_line(&grant.tool, &grant.target, workspace_root)
            })
            .collect()
    }
}

/// What `/status` reports on either surface: the configuration a turn here
/// would run under, the model it would be held in, and the conversation it
/// would continue.
///
/// `model` is a parameter rather than `config.model` because `/model` changes
/// the one and not the other: the selector holds the model in force
/// (`crate::provider::model::ModelSelector`), and a report that read the
/// configuration would name the model the session started in. No conversation
/// is zero turns and zero grants, which is what the next prompt would start
/// with.
pub(crate) fn status_snapshot(
    config: &RuntimeConfig,
    model: &str,
    conversation: Option<&Conversation>,
) -> StatusSnapshot {
    let facts = conversation.map(Conversation::facts).unwrap_or_default();
    let mut snapshot = StatusSnapshot::new(config, crate::build_info(), facts);
    snapshot.model = model.to_string();
    snapshot
}

/// Creates the session this shell records into, and the authority its tools run
/// under.
///
/// Lazily, on the first prompt: a shell that is opened and closed without asking
/// anything leaves no empty session behind, and `/new` costs nothing until it is
/// used.
///
/// **The authority is the caller's to build**, and that is the one thing this
/// function does not decide. `crate::app::permission_session` attaches a
/// prompter that reads a line from standard input, which is right for a shell
/// whose loop is the only reader of that descriptor and wrong for the TUI,
/// where the UI thread is polling it -- two readers on one terminal is the bug
/// the TUI's whole topology exists to prevent. So the caller says what may say
/// yes, and this function only binds it to the session it just created, which
/// is what makes an "always" answer durable.
pub(crate) fn open_conversation(
    store: &SessionStore,
    config: &RuntimeConfig,
    model: &str,
    permissions: PermissionSession,
    questioner: Option<Arc<dyn QuestionRequester>>,
    cancel: &CancelToken,
) -> Result<Conversation, String> {
    let scope = conversation_scope(config)?;
    let session = store
        .create(
            SessionId::generate(),
            NewSession {
                origin_workspace_root: config.workspace_root.clone(),
                workspace_root: config.workspace_root.clone(),
                model: model.to_string(),
                // The mode the authority is really in, rather than a second
                // copy of it the caller could pass a different value for.
                permission_mode: permissions.mode(),
                provider: Some(config.provider),
            },
        )
        .map_err(|err| err.to_string())?;
    Ok(bind_conversation(
        store,
        session,
        scope,
        permissions,
        questioner,
        cancel,
    ))
}

/// The scope a conversation opened under `config` runs in: the workspace,
/// and every directory `/workspace` saved for it that is still there
/// ([`crate::workspace::access`]).
///
/// One function for every place a conversation is opened -- a first prompt, a
/// `/rename` before one, a resumed session on either surface -- so a saved
/// directory cannot be in force in one of them and missing from another.
pub(crate) fn conversation_scope(config: &RuntimeConfig) -> Result<AccessScope, String> {
    WorkspaceAccess::load(&config.workspace_root, &config.additional_directories)
        .scope(&config.workspace_root)
        .map_err(|err| err.to_string())
}

/// Binds an open session to the authority its tools run under.
///
/// [`open_conversation`]'s second half, and the whole of what continuing a
/// saved session needs once `SessionStore::resume` has handed it back (the
/// TUI's `xfx --resume`, `crate::tui::worker`): the grants the session's log
/// already holds go back into the authority, as `xfx ask --resume` restores
/// them, so an "always" answered last time is not asked again. A session this
/// shell just created has none, so for it the loop adds nothing.
///
/// Infallible on purpose: the two things that can fail -- the scope and the
/// session -- are the caller's to have in hand already, which is what lets a
/// resumed session be refused before the terminal is taken.
pub(crate) fn bind_conversation(
    store: &SessionStore,
    session: WritableSession,
    scope: AccessScope,
    permissions: PermissionSession,
    questioner: Option<Arc<dyn QuestionRequester>>,
    cancel: &CancelToken,
) -> Conversation {
    let mut permissions = permissions
        .with_durable_session(session.id().as_str())
        .with_session_rules(crate::permission::session_rules::path_in(
            &store.session_dir(session.id()),
        ));
    for grant in &session.state().grants {
        permissions.grant(grant.clone());
    }
    let recorder = SessionRecorder::new(store.clone(), session);
    let mut tools = ToolContext::new(scope)
        .with_permissions(permissions)
        .with_cancel(cancel.clone());
    if let Some(questioner) = questioner {
        tools = tools.with_questioner(questioner);
    }
    Conversation {
        recorder,
        tools,
        said: std::collections::BTreeSet::new(),
        calibration: std::sync::Arc::default(),
    }
}

// ---------------------------------------------------------------------------
// the loop
// ---------------------------------------------------------------------------

/// Runs the shell until the user leaves it.
///
/// `diagnostics` receives everything the shell says before it has a terminal to
/// own -- the refusals above. Past that point the shell writes to the process
/// streams directly, because by then they are provably the terminal.
pub async fn run(
    config: &RuntimeConfig,
    diagnostics: &mut dyn Write,
) -> Result<ExitCode, AppError> {
    // Both ends, before anything else is resolved and before the profile home
    // is touched: a shell whose prompt nobody can see, or whose answers go into
    // a pipe, is a program waiting forever for a person who is not there.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        writeln!(diagnostics, "{NO_TERMINAL}")?;
        return Ok(ExitCode::from(1));
    }
    let Some(profile_dir) = config.profile_dir.clone() else {
        writeln!(diagnostics, "{NO_STORE}")?;
        return Ok(ExitCode::from(1));
    };
    // Opened now rather than at the first prompt: "xfx cannot write here" is a
    // fact about the machine, and the moment to say it is before the user has
    // typed a paragraph they are about to lose.
    let store = match SessionStore::open(&profile_dir) {
        Ok(store) => store,
        Err(err) => {
            writeln!(diagnostics, "xfx: {err}")?;
            return Ok(ExitCode::from(1));
        }
    };

    if config.permission_mode == PermissionMode::Yolo {
        writeln!(diagnostics, "{YOLO_WARNING}")?;
    }

    // Owned from here down, because `/setup` replaces it. Everything below
    // reads the configuration through this binding, so a provider switch is one
    // assignment rather than a set of fields kept in step by hand -- and the
    // value it is assigned comes from re-reading the file that was just
    // written, never from what the writer believed it wrote.
    let mut config = config.clone();
    // The configured table every conversation this shell opens decides with,
    // and the one `/allowlist` puts a changed file's table into: a handle
    // rather than a copy, so a change is in force for the next tool call.
    let rules = LiveRules::new(config.permission.effective().clone());
    let env = Environment::from_process();

    let cancel = CancelToken::new();
    let interrupts = Arc::new(Interrupts::new(cancel.clone()));
    let signal_target = Arc::clone(&interrupts);
    spawn_interrupt_thread(move || signal_target.signalled());

    let mut selector = ModelSelector::new(&config);
    let mut model = selector.model().to_string();
    let mut conversation: Option<Conversation> = None;
    // Where `/sound` stands. Read once and then moved only by `/sound`: a
    // `/setup` re-reads the configuration, and a provider switch is not a
    // reason for the sound to change under the user.
    let mut sound = config.notifications;
    // Built on first use: a shell must open on a machine with no credential --
    // that is exactly the machine whose user needs `/help` -- and it must not
    // reach for a network endpoint until there is something to send.
    let mut bundle: Option<Bundle> = None;
    // A `/permissions remember` or `revoke` waiting for its answer.
    let mut pending_rule: Option<crate::permission::slash::Confirmation> = None;

    write!(io::stdout(), "{}", banner(&config, &model))?;
    io::stdout().flush()?;

    loop {
        {
            let mut stdout = io::stdout();
            write!(stdout, "{PROMPT}")?;
            stdout.flush()?;
        }
        let Some(line) = read_line()? else {
            // End of input. The prompt is unterminated, so close its line.
            writeln!(io::stdout())?;
            return Ok(ExitCode::SUCCESS);
        };
        interrupts.line_submitted();

        // A rule change printed by `/permissions remember` or `revoke` is
        // answered by this line, whatever it says.
        if let Some(confirmation) = pending_rule.take() {
            answer_rule_confirmation(&line, &confirmation)?;
            continue;
        }

        match classify(&line) {
            Submitted::Blank => continue,
            Submitted::UnknownCommand { token } => {
                writeln!(io::stderr(), "{}", unknown_command_message(&token))?;
            }
            Submitted::Command { command, argument } => {
                match command {
                    Slash::Quit => return Ok(ExitCode::SUCCESS),
                    Slash::Help => {
                        write!(io::stdout(), "{}", help_text())?;
                    }
                    Slash::Version => {
                        writeln!(io::stdout(), "{}", version_line())?;
                    }
                    // The `xfx status` report, about this shell's own model and
                    // conversation rather than the last session on disk.
                    Slash::Status => {
                        let snapshot = status_snapshot(&config, &model, conversation.as_ref());
                        write!(io::stdout(), "{}", snapshot.render_text())?;
                    }
                    Slash::Stats => {
                        writeln!(io::stderr(), "{STATS_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Usage => {
                        writeln!(io::stderr(), "{USAGE_ON_THE_LINE_SHELL}")?;
                    }
                    // Upstream's `credits` report, `[credits] key=value` per
                    // field (`renderText`, `output_contracts.zig:1483-1511`),
                    // asked of the provider the configuration names now: a
                    // `/setup` since launch is the provider whose account this
                    // is. A refusal goes where this shell's refusals go.
                    Slash::Credits => {
                        let answer = crate::credits::fetch(&config).await;
                        if answer.is_error() {
                            write!(io::stderr(), "{}", answer.render_text())?;
                        } else {
                            write!(io::stdout(), "{}", answer.render_text())?;
                        }
                    }
                    Slash::Resume => {
                        writeln!(io::stderr(), "{RESUME_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Fast | Slash::Ultrafast => {
                        writeln!(io::stderr(), "{SPEED_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Image | Slash::Images | Slash::Paste => {
                        writeln!(io::stderr(), "{IMAGES_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Settings => {
                        writeln!(io::stderr(), "{SETTINGS_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Mcp => {
                        writeln!(io::stderr(), "{MCP_ON_THE_LINE_SHELL}")?;
                    }
                    // Upstream's sentence under its topic, in this shell's
                    // `[shell] <topic>:` shape; whatever follows the name is
                    // dropped, as upstream drops it.
                    Slash::Alias => {
                        writeln!(
                            io::stdout(),
                            "[shell] {ALIAS_TOPIC}: {ALIASES_NOT_CONFIGURABLE}"
                        )?;
                    }
                    // The session is opened for it when no prompt has opened
                    // one yet, as upstream's exists from launch: a title is
                    // something to keep, and a session renamed before anything
                    // was asked in it is kept (`discardableOnClose`,
                    // `app_session_runtime.zig:5702-5720`).
                    Slash::Rename => match validate_title(&argument) {
                        Err(problem) => {
                            writeln!(io::stderr(), "xfx: {}", title_refusal(problem))?;
                        }
                        Ok(title) => {
                            let opened = match &mut conversation {
                                Some(existing) => Ok(existing),
                                slot @ None => open_conversation(
                                    &store,
                                    &config,
                                    &model,
                                    crate::app::permission_session(
                                        config.permission_mode,
                                        rules.clone(),
                                    ),
                                    None,
                                    &cancel,
                                )
                                .map(|opened| slot.insert(opened)),
                            };
                            match opened.and_then(|open| rename_conversation(open, title)) {
                                Ok(()) => writeln!(
                                    io::stdout(),
                                    "[shell] session renamed to \"{title}\""
                                )?,
                                Err(problem) => writeln!(
                                    io::stderr(),
                                    "xfx: the session was not renamed: {problem}"
                                )?,
                            }
                        }
                    },
                    Slash::Model => {
                        model =
                            apply_model(&argument, &mut selector, conversation.as_mut()).await?;
                    }
                    Slash::Setup => {
                        model = apply_setup(
                            &argument,
                            &env,
                            &mut config,
                            &mut selector,
                            &mut bundle,
                            &mut conversation,
                        )
                        .await?;
                    }
                    // One operation under three names, as upstream's are
                    // (`app_commands.zig:667-680`): `/clear` no longer erases
                    // anything -- what the shell printed stays where the user
                    // can scroll back to it, which is upstream's scrollback
                    // handoff on a surface that has no screen of its own.
                    Slash::Clear | Slash::New | Slash::Reset => {
                        // Dropping the recorder closes the log and releases the
                        // session's writer lock, so the next prompt is free to
                        // create a genuinely new identity.
                        conversation = None;
                        writeln!(
                            io::stdout(),
                            "[shell] new session; the next prompt starts a fresh conversation"
                        )?;
                    }
                    Slash::Copy => copy_last_reply(conversation.as_ref(), &Clipboard::system())?,
                    Slash::Undo => undo_on_the_line(conversation.as_ref())?,
                    Slash::Trace => trace_on_the_line(
                        &config,
                        &model,
                        conversation.as_ref(),
                        &store,
                        &TraceTarget {
                            dir: crate::trace::report_dir(std::env::var_os("TMPDIR").as_deref()),
                            clipboard: Clipboard::system(),
                        },
                        &mut io::stdout(),
                        &mut io::stderr(),
                    )?,
                    Slash::Compact => {
                        writeln!(io::stderr(), "{COMPACT_ON_THE_LINE_SHELL}")?;
                    }
                    Slash::Permissions => {
                        apply_permissions(
                            &argument,
                            &mut config,
                            conversation.as_mut(),
                            &rules,
                            &mut pending_rule,
                        )?;
                    }
                    Slash::Allowlist => allowlist_on_the_line(
                        &argument,
                        &config,
                        &rules,
                        &mut io::stdout(),
                        &mut io::stderr(),
                    )?,
                    Slash::Statusline => statusline_on_the_line(
                        &argument,
                        &mut config,
                        &mut io::stdout(),
                        &mut io::stderr(),
                    )?,
                    Slash::Sound => {
                        sound =
                            apply_sound(&argument, sound, config.user_settings_path.as_deref())?;
                    }
                    Slash::Skills => skills_on_the_line(
                        &argument,
                        &config,
                        &mut io::stdout(),
                        &mut io::stderr(),
                    )?,
                    Slash::Workspace => workspace_on_the_line(
                        &argument,
                        &mut config,
                        conversation.as_mut(),
                        &mut io::stdout(),
                        &mut io::stderr(),
                    )?,
                }
                io::stdout().flush()?;
            }
            Submitted::Prompt(prompt) => {
                let bundle_ref = match ensure_provider(&mut bundle, &config, &cancel) {
                    Ok(bundle) => bundle,
                    Err(message) => {
                        report_turn_failure(message)?;
                        continue;
                    }
                };
                let conversation = match &mut conversation {
                    Some(existing) => existing,
                    slot @ None => match open_conversation(
                        &store,
                        &config,
                        &model,
                        // The configuration's mode rather than the launch's:
                        // `/permissions` moves it, and a conversation opened
                        // after that is opened in the mode the user chose.
                        crate::app::permission_session(config.permission_mode, rules.clone()),
                        // fx's interactive shell is xfx's TUI, not this line
                        // shell: this loop reads one line at a time from
                        // standard input and has nowhere to render a question
                        // panel, so it never has a requester and
                        // `ask_user_question` answers with
                        // `NOT_AVAILABLE_SENTINEL` here.
                        None,
                        &cancel,
                    ) {
                        Ok(opened) => slot.insert(opened),
                        Err(message) => {
                            report_turn_failure(format!("xfx: {message}"))?;
                            continue;
                        }
                    },
                };
                one_turn(
                    bundle_ref.stream.as_ref(),
                    conversation,
                    &model,
                    prompt,
                    &config,
                    &interrupts,
                )
                .await?;
            }
        }
    }
}

/// Runs exactly one turn, the same way `xfx ask` runs its one.
async fn one_turn(
    provider: &dyn Provider,
    conversation: &mut Conversation,
    model: &str,
    prompt: String,
    config: &RuntimeConfig,
    interrupts: &Interrupts,
) -> io::Result<()> {
    // Project instructions are read now rather than remembered, so editing
    // `AGENTS.md` in another window takes effect on the next prompt.
    let context = ProjectContext::discover(conversation.tools.scope());
    conversation
        .recorder
        .commit(SessionEvent::ProjectContextRecorded {
            sources: context
                .sources()
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            bytes: context.total_bytes() as u64,
        });

    // The skills this workspace has, and those the prompt names, read now as
    // the project instructions are (`crate::skills`). This shell reads no
    // catalog for a turn, so the catalog's budget is sized by the window the
    // Gateway is known to give the model, and by none elsewhere.
    let window = (config.provider == ProviderId::Gateway)
        .then(|| crate::provider::model::gateway_context_window(model))
        .flatten();
    let skills = crate::skills::TurnSkills::prepare(
        conversation.tools.scope().primary(),
        config.home(),
        &prompt,
        &[],
        window,
    );
    let mut tools = conversation
        .tools
        .clone()
        .with_link_bases(crate::permission::LinkBase::from_environment())
        .with_git_environment(crate::permission::GitEnvironment::from_process());
    tools.set_skills(
        Some(std::sync::Arc::new(skills.catalog.locations.clone())),
        config.home().map(std::path::Path::to_path_buf),
    );

    let replay = conversation
        .recorder
        .state()
        .history_messages(config.provider.wire(), model);
    let request = TurnRequest {
        images: Vec::new(),
        model: model.to_string(),
        prompt,
        history: replay.messages,
        max_steps: config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        cancel: conversation.tools.cancel().clone(),
        tools,
    };

    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stderr_lock = stderr.lock();
    for notice in conversation.unsaid(replay.notices) {
        writeln!(stderr_lock, "{notice}")?;
    }
    drop(stderr_lock);
    // What the TUI says under the card, one `[shell]` line per row: which of
    // the named skills loaded (`buildExplicitPromptSection`'s `load_notice`).
    if let Some((_, said)) = &skills.explicit.load_notice {
        let mut out = io::stdout().lock();
        for line in said.lines() {
            writeln!(out, "[shell] {line}")?;
        }
    }
    let mut sink = TextSink::new(stdout, stderr).with_tool_notices();

    interrupts.begin_turn();
    // Awaited here, on this thread: when this returns there is no work left
    // running anywhere, which is what makes the next prompt safe to print.
    let _ = run_turn_with_skills(
        request,
        context,
        &skills,
        provider,
        &mut sink,
        &mut conversation.recorder,
    )
    .await;
    interrupts.end_turn();

    // Approvals given during the turn become durable once, after it, so an
    // "always" answer survives to the next `xfx ask --resume-id <id>`. The step
    // is the recorder's ([`SessionRecorder::record_new_grants`]), which is what
    // keeps this shell, `app::ask` and the TUI's worker from drifting into
    // three meanings of the same sentence.
    let granted = conversation.tools.permissions().grants().to_vec();
    conversation.recorder.record_new_grants(&granted);
    if let Some(failure) = conversation.recorder.failure() {
        writeln!(io::stderr(), "xfx: {failure}")?;
    }
    Ok(())
}

/// Builds the bundle once, or explains why there can not be one.
///
/// The decision itself belongs to [`Bundle::select`], which `ask` uses too: the
/// shell caches the result for the life of the session, it does not choose a
/// backend of its own.
fn ensure_provider<'a>(
    slot: &'a mut Option<Bundle>,
    config: &RuntimeConfig,
    cancel: &CancelToken,
) -> Result<&'a Bundle, String> {
    if slot.is_none() {
        *slot = Some(Bundle::select(config, cancel)?);
    }
    Ok(slot.as_ref().expect("the bundle was just built"))
}

/// Reports a prompt that never became a request, in the shape a turn failure has.
fn report_turn_failure(message: String) -> io::Result<()> {
    let mut sink = TextSink::new(io::stdout(), io::stderr());
    sink.emit(&Event::Error { message })
}

/// Reads one line, or `None` at end of input.
///
/// The process-wide buffered stdin is used rather than a private reader,
/// because the permission prompt reads from the same terminal in the middle of
/// a turn: two buffers over one descriptor would let one of them swallow the
/// other's answer.
fn read_line() -> io::Result<Option<String>> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(line)),
        // Not fatal: the bytes that were not text are gone, and the shell says
        // so and reads the next line rather than exiting on a stray paste.
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            writeln!(io::stderr(), "xfx: that line was not valid UTF-8; ignored")?;
            Ok(Some(String::new()))
        }
        Err(err) => Err(err),
    }
}

/// Gets the label for a provider id.
fn provider_label(provider: crate::provider::ProviderId) -> &'static str {
    match provider {
        crate::provider::ProviderId::Gateway => "gateway",
        crate::provider::ProviderId::Llmux => "llmux",
    }
}

/// Prints the catalog to stdout, bounded at MAX_RENDERED_MODELS.
fn print_catalog(catalog: &crate::provider::model::CatalogState) -> io::Result<()> {
    use crate::provider::model::{CatalogState, MAX_RENDERED_MODELS};

    match catalog {
        CatalogState::Unavailable => {
            writeln!(
                io::stdout(),
                "{}",
                crate::provider::model::NO_CATALOG_NOTICE
            )?;
        }
        CatalogState::NotLoaded => {
            // Not printed if not loaded yet.
        }
        CatalogState::Loaded(entries) => {
            writeln!(io::stdout(), "[shell] catalog={} models", entries.len())?;
            for entry in entries.iter().take(MAX_RENDERED_MODELS) {
                let context_str = entry
                    .max_context
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let efforts_str = if entry.efforts.is_empty() {
                    "none".to_string()
                } else {
                    entry.efforts.join(",")
                };
                writeln!(
                    io::stdout(),
                    "[shell]   {} context={} efforts={}",
                    entry.preferred_name(),
                    context_str,
                    efforts_str
                )?;
            }
            let remaining = entries.len().saturating_sub(MAX_RENDERED_MODELS);
            if remaining > 0 {
                writeln!(io::stdout(), "[shell]   ... and {remaining} more")?;
            }
        }
        CatalogState::Failed(reason) => {
            writeln!(io::stdout(), "[shell] catalog=unread ({reason})")?;
        }
    }
    Ok(())
}

/// What `/setup` says when it was not given a provider it can set up.
///
/// Derived from [`ProviderId::ALL`] rather than spelled, for the reason
/// [`unknown_command_message`] derives its count: a provider added to the build
/// and not to this sentence would be a provider the shell can switch to and
/// will not admit to.
pub fn setup_usage() -> String {
    let names: Vec<&str> = ProviderId::ALL.iter().map(|id| id.label()).collect();
    format!(
        "xfx: /setup takes a provider to set up -- {}",
        names.join(" or ")
    )
}

/// Applies `/setup <provider>`, returning the model in force afterwards.
///
/// The command's whole shape is that the **file** decides, not this function:
/// the transaction writes `~/.xfx/settings.json` through the same
/// `provider::setup::run` the `xfx setup` subcommand runs -- so the on-disk
/// bytes are the same bytes -- and then the configuration is **re-read from
/// that file**. Nothing here assigns a provider, a model or a URL from what the
/// writer believed it wrote; a layer that outranks the profile still outranks it
/// after the write, and the report says which one.
///
/// The bundle goes with it: it is a connection to the endpoint that is no
/// longer the one configured. **The conversation stays**, as the TUI's does
/// and as upstream's `/provider` keeps its session (`vercel-labs/fx@c1db9191
/// src/core/app/app_auth_runtime.zig:1377-1381`): its history is rebuilt from
/// the log for each request in the wire then in force, and the switch is
/// written into that log, provider and model both
/// (`app_session_runtime.zig:3539-3550`).
async fn apply_setup(
    argument: &str,
    env: &Environment,
    config: &mut RuntimeConfig,
    selector: &mut ModelSelector,
    bundle: &mut Option<Bundle>,
    conversation: &mut Option<Conversation>,
) -> io::Result<String> {
    let Some(provider) = ProviderId::parse(argument) else {
        writeln!(io::stderr(), "{}", setup_usage())?;
        return Ok(selector.model().to_string());
    };
    let report = match crate::provider::setup::run(config, env, provider, None).await {
        Ok(report) => report,
        Err(err) => {
            // The old provider is kept, whole: nothing was written, so nothing
            // about this session has changed.
            writeln!(io::stderr(), "xfx: {err}")?;
            return Ok(selector.model().to_string());
        }
    };
    let settings_path = report.settings_path.clone();
    let snapshot = crate::output::SetupSnapshot::new(&report);
    write!(io::stdout(), "{}", snapshot.render(OutputFormat::Text))?;
    if let Some(warning) = snapshot.override_warning() {
        writeln!(io::stderr(), "{warning}")?;
    }
    if let Some(warning) = snapshot.credential_warning() {
        writeln!(io::stderr(), "{}", warning)?;
    }

    // Re-read, rather than believed. `load_with` is the only thing in the
    // product that decides what a settings file means, and a shell that set the
    // fields itself would be a second such thing -- one that cannot see the
    // environment override the report just warned about.
    match RuntimeConfig::load_with(env, &config.workspace_root) {
        Ok(reloaded) => {
            *config = reloaded;
            *selector = ModelSelector::new(config);
            *bundle = None;
            let model = selector.model().to_string();
            if let Some(conversation) = conversation.as_mut() {
                conversation
                    .recorder
                    .commit(SessionEvent::PreferencesChanged {
                        model: Some(model.clone()),
                        permission_mode: None,
                        provider: Some(config.provider.label().to_string()),
                        effort: None,
                        fast_mode: None,
                        ultrafast_mode: None,
                    });
            }
            writeln!(
                io::stdout(),
                "[shell] provider={} model={model}",
                config.provider.label()
            )?;
            Ok(model)
        }
        Err(err) => {
            // The file on disk is the new one and this process cannot read it.
            // Saying so is the whole of what is left to do: inventing the
            // provider from the report would be running against a
            // configuration nothing has parsed.
            writeln!(
                io::stderr(),
                "xfx: {} was written but could not be re-read: {err}",
                settings_path.display()
            )?;
            Ok(selector.model().to_string())
        }
    }
}

/// Applies `/model`, returning the model in force afterwards.
///
/// The rules live in [`crate::provider::model::ModelSelector`] so the shell and
/// any other front end cannot disagree about what `/model` means; what stays
/// here is the printing, which is this surface's own business.
async fn apply_model(
    argument: &str,
    selector: &mut ModelSelector,
    conversation: Option<&mut Conversation>,
) -> io::Result<String> {
    if argument.is_empty() {
        // The one place `/model` is allowed to touch the network: the catalog
        // load is where a provider that is not answering legitimately surfaces,
        // and `status` and `doctor` are where it must not.
        selector.ensure_catalog().await;
    }
    match selector.apply(if argument.is_empty() {
        ModelRequest::Report
    } else {
        ModelRequest::Select(argument)
    }) {
        ModelOutcome::Reported {
            provider,
            model,
            source,
        } => {
            writeln!(
                io::stdout(),
                "[shell] model={model} provider={} source={}",
                provider_label(provider),
                source.label()
            )?;
            print_catalog(selector.catalog())?;
            Ok(model)
        }
        ModelOutcome::Selected {
            model,
            previous: _,
            unverified,
            ..
        } => {
            writeln!(io::stdout(), "[shell] model={model}")?;
            if let Some(reason) = unverified {
                // The sentence is shared with the band
                // (`provider::model::unverified_notice`); where it is written
                // is this surface's own business.
                writeln!(
                    io::stderr(),
                    "{}",
                    crate::provider::model::unverified_notice(&reason)
                )?;
            }
            // Durable when there is something to record it in: a resumed session must
            // continue with the model the conversation was actually held in.
            if let Some(conversation) = conversation {
                conversation
                    .recorder
                    .commit(SessionEvent::PreferencesChanged {
                        model: Some(model.clone()),
                        permission_mode: None,
                        provider: None,
                        effort: None,
                        fast_mode: None,
                        ultrafast_mode: None,
                    });
            }
            Ok(model)
        }
        ModelOutcome::Unchanged { model } => {
            writeln!(io::stdout(), "[shell] model={model} unchanged")?;
            Ok(model)
        }
        ModelOutcome::Refused { reason } => {
            writeln!(io::stderr(), "xfx: {reason}")?;
            Ok(selector.model().to_string())
        }
    }
}

/// Applies `/permissions`, with the TUI's meaning in this shell's lines.
///
/// The words are [`crate::permission::slash`]'s, so the two surfaces say the
/// same thing; the shape is this shell's: fx's own line format for its
/// `permissions` command (`[permissions] mode=…`, `renderText`,
/// `output_contracts.zig:705-722`) on stdout, and the usage line and a failed
/// write on stderr. Every command here runs between turns -- the loop runs one
/// thing at a time -- so a change is applied at once, to the configuration a
/// later conversation is opened from and to the conversation in hand, and
/// recorded in the profile the next launch reads.
///
/// `remember` and `revoke` are confirmed on the next line this shell reads
/// (`pending`): the question, the line under it and the action are printed as
/// `[permissions]` lines with the two choices, and `1` confirms -- anything
/// else is the second choice, `Cancel` ([`answer_rule_confirmation`]).
fn apply_permissions(
    argument: &str,
    config: &mut RuntimeConfig,
    conversation: Option<&mut Conversation>,
    rules: &LiveRules,
    pending: &mut Option<crate::permission::slash::Confirmation>,
) -> io::Result<()> {
    use crate::permission::slash::{self, Managed, Request};
    let request = Request::parse(argument);
    let rules_path = conversation.as_ref().and_then(|conversation| {
        conversation
            .tools
            .permissions()
            .session_rules_path()
            .map(std::path::Path::to_path_buf)
    });
    let managed = slash::manage(&request, rules_path.as_deref(), false, |tool, arguments| {
        // The session's own scope, added directories and all, as upstream
        // prepares a rule (`tool_admission.zig:2270-2289`).
        let scope = conversation_scope(config)?;
        let context = ToolContext::new(scope).with_permissions(
            // `ask`, whatever the session's mode, as upstream prepares a rule
            // (`.permission_mode = .ask`, `tool_admission.zig:2286`): in full
            // access no table is consulted, and a change a configured rule
            // denies must not be remembered as allowed.
            PermissionSession::new(PermissionMode::Ask).with_live_rules(rules.clone()),
        );
        crate::tools::prepare_rule_action(tool, arguments, &context)
    });
    if let Some(managed) = managed {
        match managed {
            Managed::Say { tone, topic, body } => say_permission_line(tone, topic, &body)?,
            Managed::Confirm(confirmation) => {
                let mut stdout = io::stdout();
                writeln!(stdout, "[permissions] {}", confirmation.question)?;
                writeln!(stdout, "[permissions] {}", confirmation.explanation)?;
                writeln!(
                    stdout,
                    "  $ {}",
                    safe_one_line(&confirmation.display, usize::MAX)
                )?;
                writeln!(stdout, "  1. Confirm")?;
                writeln!(stdout, "  2. Cancel")?;
                *pending = Some(confirmation);
            }
        }
        return Ok(());
    }
    let (mode, reset) = match request {
        Request::Remember { .. } | Request::Revoke(_) | Request::ManagementUsage => {
            unreachable!("`manage` answers every rule-management form")
        }
        Request::Show => {
            let grants = conversation
                .map(|conversation| conversation.grant_lines(&config.workspace_root))
                .unwrap_or_default();
            let mut stdout = io::stdout();
            // fx's own rows: the facts behind the prefix and each grant under
            // them as ` - <tool> -> <target>`, flattened to one line because a
            // command a model wrote is not text this shell hands a terminal.
            let rules = rules.current();
            for row in slash::status_rows(config.permission_mode, &rules, &grants).lines() {
                match row.strip_prefix(" - ") {
                    Some(grant) => writeln!(stdout, " - {}", safe_one_line(grant, usize::MAX))?,
                    None => writeln!(stdout, "[permissions] {row}")?,
                }
            }
            match slash::saved_rules_at(rules_path.as_deref()) {
                Ok(body) => {
                    for (index, row) in body.lines().enumerate() {
                        let row = safe_one_line(row, usize::MAX);
                        if index == 0 {
                            writeln!(stdout, "[permissions] {row}")?;
                        } else {
                            writeln!(stdout, " {row}")?;
                        }
                    }
                }
                Err(reason) => writeln!(io::stderr(), "xfx: permissions: {reason}")?,
            }
            for row in slash::USAGE.lines() {
                writeln!(stdout, "[permissions] {}", row.trim_start())?;
            }
            return Ok(());
        }
        Request::Usage => {
            for row in slash::USAGE.lines() {
                writeln!(io::stderr(), "xfx: {}", row.trim_start())?;
            }
            return Ok(());
        }
        Request::Select(mode) => (mode, false),
        Request::Reset => (PermissionMode::Ask, true),
    };
    config.permission_mode = mode;
    if let Some(conversation) = conversation {
        conversation.set_permission_mode(mode, reset);
    }
    // Recorded before it is announced, as upstream's `selectMode` persists
    // before `handlePermissions` writes its notice: a line saying the mode
    // changed is a line a reader may act on, the file included.
    let written = match config.user_settings_path.as_ref() {
        Some(path) => crate::provider::profile::write_preference(
            path,
            &["permission_mode"],
            serde_json::Value::String(mode.label().to_string()),
        )
        .map_err(|err| err.to_string()),
        None => Err("no home directory".to_string()),
    };
    let said = if reset {
        slash::RESET.to_string()
    } else {
        slash::selected(mode)
    };
    writeln!(io::stdout(), "[permissions] {said}")?;
    if mode == PermissionMode::Yolo {
        writeln!(io::stderr(), "{YOLO_WARNING}")?;
    }
    match written {
        // A layer above the profile still decides what a fresh session here
        // starts in: said, in upstream's words, on the stream warnings go to.
        Ok(()) => {
            if let Some(said) = crate::provider::profile::saved_but_shadowed(
                "permission_mode",
                config.sources.permission_mode,
            ) {
                writeln!(io::stderr(), "xfx: permission-mode: {said}")?;
            }
        }
        Err(reason) => writeln!(
            io::stderr(),
            "xfx: permission-mode: {}",
            slash::not_saved(&reason)
        )?,
    }
    Ok(())
}

/// `/statusline` on this shell.
///
/// There is no footer here to draw the segments in and no band to put a picker
/// in, so this surface does the half of the command that outlives it: an item
/// named is flipped and kept in the profile the full-screen shell reads at
/// launch, in the same two sentences that shell says. Bare, it says what each
/// item is set to and how to change one -- the picker's content, as lines.
fn statusline_on_the_line(
    argument: &str,
    config: &mut RuntimeConfig,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    if argument.is_empty() {
        let states: Vec<String> = StatusLineItem::ALL
            .into_iter()
            .map(|item| statusline_state(item, config.statusline.get(item)))
            .collect();
        writeln!(out, "[shell] {STATUSLINE_TOPIC}: {}", states.join(", "))?;
        return writeln!(out, "[shell] {STATUSLINE_USAGE}");
    }
    let Some(item) = StatusLineItem::parse(argument) else {
        return writeln!(err, "xfx: {STATUSLINE_USAGE}");
    };
    let on = !config.statusline.get(item);
    config.statusline.set(item, on);
    match save_statusline(config.user_settings_path.as_deref(), item, on) {
        Ok(()) => writeln!(out, "[shell] {STATUSLINE_TOPIC}: {STATUSLINE_SAVED}")?,
        Err(reason) => writeln!(
            err,
            "xfx: {STATUSLINE_TOPIC}: {}",
            statusline_not_saved(&reason)
        )?,
    }
    writeln!(
        out,
        "[shell] {STATUSLINE_TOPIC}: {}",
        statusline_state(item, on)
    )
}

/// `/workspace` on this shell.
///
/// There is no band to put upstream's picker in, so a bare `/workspace` is
/// `/workspace list`: the snapshot, its rows under one `[shell] workspace:`
/// line. A change is made at once -- this loop runs no turn while it reads a
/// line, which is the whole of upstream's "between turns" rule here -- and an
/// open conversation is put in the new scope before the next prompt is read.
/// A refusal is upstream's sentence on standard error.
fn workspace_on_the_line(
    argument: &str,
    config: &mut RuntimeConfig,
    conversation: Option<&mut Conversation>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    let primary = config.workspace_root.clone();
    let current = WorkspaceAccess::load(&primary, &config.additional_directories);
    let action = match workspace_access::parse(argument) {
        Err(()) => return writeln!(err, "xfx: {}", workspace_access::USAGE),
        Ok(None) => {
            let body = workspace_access::snapshot(&primary, &current.refreshed(), None);
            return workspace_said(out, &body);
        }
        Ok(Some(action)) => action,
    };
    let updated = match workspace_access::execute(
        &primary,
        config.user_settings_path.as_deref(),
        &current,
        &action,
    ) {
        Ok(updated) => updated,
        Err(failure) => {
            return writeln!(
                err,
                "xfx: {}: {}",
                workspace_access::TOPIC,
                failure.message()
            )
        }
    };
    config.additional_directories = updated.access.saved();
    if let Some(conversation) = conversation {
        match updated.access.scope(&primary) {
            Ok(scope) => conversation.tools.set_scope(scope),
            Err(problem) => writeln!(err, "xfx: {}: {problem}", workspace_access::TOPIC)?,
        }
    }
    workspace_said(
        out,
        &workspace_access::snapshot(&primary, &updated.access, Some(&updated.mutation)),
    )
}

/// `/skills` on this shell: the forms the TUI answers with notices answered
/// the same way, as `[shell] skills: …` lines; and the two that open the
/// TUI's menu -- a bare `/skills` or `list`, and `show` -- said as upstream's
/// listing (`listSkillsSummary`, `vercel-labs/fx@c1db9191
/// src/core/skills/skill_runtime.zig:2964-3004`), there being no band to put a
/// menu in.
fn skills_on_the_line(
    argument: &str,
    config: &RuntimeConfig,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    use crate::skills::command::{self, Command, Outcome};
    let workspace = &config.workspace_root;
    let found = crate::skills::discovery::discover(workspace, config.home());
    if !found.diagnostics.is_empty() {
        writeln!(
            err,
            "xfx: {}: {}",
            command::TOPIC,
            crate::skills::diagnostic_summary(&found.diagnostics)
        )?;
    }
    let Some(home) = config.home() else {
        return writeln!(
            err,
            "xfx: {}: no home directory is set, so there is no managed skill root",
            command::TOPIC
        );
    };
    let managed = crate::skills::discovery::managed_root(home);
    let parsed = command::parse(argument);
    let said = |out: &mut dyn Write, body: &str| -> io::Result<()> {
        let topic = command::topic_for(body);
        for (index, line) in body.lines().enumerate() {
            match (index, topic.is_empty()) {
                (0, true) => writeln!(out, "[shell] {line}")?,
                (0, false) => writeln!(out, "[shell] {topic}: {line}")?,
                _ => writeln!(out, "[shell]   {line}")?,
            }
        }
        Ok(())
    };
    match command::execute(&parsed, &found, &managed, workspace) {
        Outcome::OpenMenu => {
            for line in crate::skills::summary(&found.skills).lines() {
                writeln!(out, "[shell] {line}")?;
            }
            Ok(())
        }
        Outcome::Focus(name) | Outcome::Search(name) => {
            let named: Vec<crate::skills::Skill> = found
                .skills
                .iter()
                .filter(|skill| skill.name == name)
                .cloned()
                .collect();
            for line in crate::skills::summary(&named).lines() {
                writeln!(out, "[shell] {line}")?;
            }
            Ok(())
        }
        Outcome::Said(lines) => {
            for body in &lines {
                if matches!(parsed, Command::Usage) {
                    writeln!(err, "xfx: {body}")?;
                } else {
                    said(out, body)?;
                }
            }
            Ok(())
        }
        // No list held between lines here: the next command discovers again,
        // so what changed is said with the rest.
        Outcome::Changed { said: lines, after } => {
            for body in lines.iter().chain(std::iter::once(&after)) {
                said(out, body)?;
            }
            Ok(())
        }
    }
}

/// A `/workspace` snapshot on this shell: the first row after the topic, the
/// rest two columns in, as the TUI's notice continues its rows.
fn workspace_said(out: &mut dyn Write, body: &str) -> io::Result<()> {
    for (index, line) in body.lines().enumerate() {
        if index == 0 {
            writeln!(out, "[shell] {}: {line}", workspace_access::TOPIC)?;
        } else {
            writeln!(out, "[shell]   {line}")?;
        }
    }
    Ok(())
}

/// One rule-management answer on this shell: a neutral or warning line on
/// stdout, an error on stderr, each row made safe.
fn say_permission_line(
    tone: crate::permission::allowlist::Tone,
    topic: &str,
    body: &str,
) -> io::Result<()> {
    use crate::permission::allowlist::Tone;
    let topic = if topic.is_empty() {
        String::new()
    } else {
        format!("{topic}: ")
    };
    for (index, row) in body.lines().enumerate() {
        let row = safe_one_line(row.trim_start(), usize::MAX);
        let lead = if index == 0 { topic.as_str() } else { "" };
        match tone {
            Tone::Error => writeln!(io::stderr(), "xfx: {lead}{row}")?,
            Tone::Neutral | Tone::Warning => writeln!(io::stdout(), "[permissions] {row}")?,
        }
    }
    Ok(())
}

/// The line read while a rule change waits: `1` makes it and says how it went
/// ([`crate::permission::slash::confirm`]); anything else is `Cancel`.
fn answer_rule_confirmation(
    line: &str,
    confirmation: &crate::permission::slash::Confirmation,
) -> io::Result<()> {
    use crate::permission::slash;
    if line.trim() == "1" {
        let (tone, body) = slash::confirm(confirmation);
        say_permission_line(tone, slash::TOPIC, &body)
    } else {
        writeln!(io::stdout(), "[permissions] {}", slash::CANCELLED)
    }
}

/// `/allowlist` on this shell: upstream's forms and words
/// ([`crate::permission::allowlist`]), each answer one `[allowlist]` line on
/// stdout -- a listing's further rows under it as they are -- and a refusal
/// or a failure on stderr. A table read back from the file is put in force
/// for this shell's next tool call through `rules`, as the TUI's is.
fn allowlist_on_the_line(
    argument: &str,
    config: &RuntimeConfig,
    rules: &LiveRules,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    use crate::permission::allowlist::{self, Tone};
    let outcome = allowlist::run(
        argument,
        config.user_settings_path.as_deref(),
        &config.workspace_root,
        |name| crate::tools::ADVERTISED_TOOLS.contains(&name),
    );
    if let Some(table) = outcome.in_force {
        rules.replace(table);
    }
    for answer in outcome.answers {
        let topic = if answer.topic.is_empty() {
            String::new()
        } else {
            format!("{}: ", answer.topic)
        };
        // A pattern is the person's own text, and a table can be edited by
        // hand: each row goes out the way a grant's does, one line, made safe.
        let mut rows = answer.body.lines();
        let first = safe_one_line(rows.next().unwrap_or_default(), usize::MAX);
        match answer.tone {
            Tone::Error => writeln!(err, "xfx: {topic}{first}")?,
            Tone::Neutral | Tone::Warning => writeln!(out, "[allowlist] {first}")?,
        }
        for row in rows {
            let indent = row.len() - row.trim_start_matches(' ').len();
            let row = format!(
                "{}{}",
                " ".repeat(indent),
                safe_one_line(row.trim_start_matches(' '), usize::MAX)
            );
            match answer.tone {
                Tone::Error => writeln!(err, "{row}")?,
                Tone::Neutral | Tone::Warning => writeln!(out, "{row}")?,
            }
        }
    }
    Ok(())
}

/// `/copy` on this shell: the conversation's last reply onto the clipboard,
/// and one line saying how that went ([`crate::clipboard`]).
///
/// The copy waits for the clipboard tool to exit, as upstream's does. This
/// loop is waiting for the next line anyway; it is the TUI, whose thread holds
/// a terminal that has to keep being painted, that hands the wait to a thread.
fn copy_last_reply(conversation: Option<&Conversation>, clipboard: &Clipboard) -> io::Result<()> {
    let reply = conversation.and_then(|conversation| last_reply(conversation.recorder.state()));
    match reply {
        None => writeln!(io::stdout(), "[shell] {NO_REPLY}"),
        Some(reply) if clipboard.copy(reply) => writeln!(io::stdout(), "[shell] {COPIED}"),
        Some(_) => writeln!(io::stderr(), "xfx: {COPY_FAILED}"),
    }
}

/// `/undo` on this shell: the conversation's newest change taken back, and
/// upstream's sentence for what became of it, on this loop's thread like
/// everything else it does. No conversation yet is no change yet.
fn undo_on_the_line(conversation: Option<&Conversation>) -> io::Result<()> {
    let undone = match conversation {
        Some(conversation) => crate::tools::mutate::undo_last(&conversation.tools),
        None => crate::tools::undo::Undone::Nothing,
    };
    writeln!(
        io::stdout(),
        "[shell] {}: {}",
        crate::tools::undo::TOPIC,
        undone.sentence()
    )
}

/// Where `/trace` puts its report: the directory the file is written in, and
/// the clipboard the file is copied to.
pub(crate) struct TraceTarget {
    pub(crate) dir: std::path::PathBuf,
    pub(crate) clipboard: Clipboard,
}

/// `/trace` on this shell: the report built from this shell's own state,
/// written and copied inline -- everything this loop does is inline, and a
/// clipboard tool that never answers is killed at its deadline -- and one line
/// saying how it went, on standard output when there is a report to paste or
/// open and on the diagnostic stream when the copy or the write failed, as
/// `/copy`'s lines are split here.
///
/// The session is the live conversation's, read from the recorder's own state
/// rather than back from disk: on this surface that state is in hand. Before
/// the first prompt there is none, and the report says so. The report has no
/// screen to describe, so it has no timeline and no network records -- the
/// full-screen shell's full transcript is where those are kept.
fn trace_on_the_line(
    config: &RuntimeConfig,
    model: &str,
    conversation: Option<&Conversation>,
    store: &SessionStore,
    target: &TraceTarget,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    let status = status_snapshot(config, model, conversation);
    let settings = crate::trace::Settings::of(config);
    let dir = conversation.map(|conversation| store.session_dir(conversation.recorder.id()));
    let session = match (conversation, &dir) {
        (Some(conversation), Some(dir)) => crate::trace::Session::Read {
            dir,
            state: conversation.recorder.state(),
            failure: conversation.recorder.failure(),
        },
        _ => crate::trace::Session::None,
    };
    let now = crate::session::event::system_now_ms();
    let compaction = crate::agent::compaction_trace::snapshot();
    let report = crate::trace::Report {
        generated_ms: now,
        status: &status,
        effort: Some(config.effort.label()),
        settings: &settings,
        session,
        screen: None,
        compaction: &compaction,
        title: None,
    }
    .render();
    let disposition = crate::trace::deliver(
        &report,
        &target.dir,
        &target.clipboard,
        now,
        &crate::clipboard::Stop::default(),
    );
    if disposition.failed() {
        writeln!(err, "xfx: {}", disposition.sentence())
    } else {
        writeln!(out, "[shell] {}", disposition.sentence())
    }
}

/// What this shell adds to `/sound`'s answer: that the switch it just moved is
/// the full-screen shell's to ring.
///
/// The bell is a byte the TUI writes through its checked frame writer, at a
/// turn's end and when it stops to ask -- and this shell's question is asked by
/// the prompter `xfx ask` shares, which rings nothing. So the command saves the
/// same switch to the same file and says so, the way `/stats` here says where
/// its counters are rather than printing zeros.
pub const SOUND_ON_THE_LINE_SHELL: &str =
    "the full-screen shell rings the terminal bell, and this line-oriented one rings none";

/// `/sound` on this shell: the switch moved, saved, and said.
///
/// Saved inline, because everything this loop does is inline -- it waits on
/// `/setup`'s socket the same way -- and in upstream's order: a save that
/// failed is said first, on the diagnostic stream, and the level after it.
fn apply_sound(
    argument: &str,
    current: crate::config::Notifications,
    settings: Option<&std::path::Path>,
) -> io::Result<crate::config::Notifications> {
    let Some(request) = crate::sound::Request::parse(argument) else {
        writeln!(io::stderr(), "xfx: {}", crate::sound::USAGE)?;
        return Ok(current);
    };
    let level = request.level(current);
    let next = level.notifications();
    let saved = match settings {
        Some(path) => crate::sound::save(path, level).map_err(|err| err.to_string()),
        // Not reachable from `run`, which refuses to open without a home; said
        // rather than assumed all the same.
        None => Err("no home directory is set".to_string()),
    };
    if let Err(reason) = saved {
        writeln!(
            io::stderr(),
            "xfx: {}: {}",
            crate::sound::TOPIC,
            crate::sound::not_saved(next != current, &reason)
        )?;
    }
    writeln!(
        io::stdout(),
        "[shell] {}: {}; {SOUND_ON_THE_LINE_SHELL}",
        crate::sound::TOPIC,
        level.label()
    )?;
    Ok(next)
}

/// The version line `/version` prints.
///
/// Visible to the crate because the TUI answers the same commands from the
/// same declarations rather than growing a second `/version` that could drift
/// from this one (`crate::tui::shell`).
pub(crate) fn version_line() -> String {
    let build = crate::build_info();
    match build.revision {
        Some(revision) => format!(
            "xfx {} ({}, revision {revision})",
            crate::VERSION,
            build.channel
        ),
        None => format!("xfx {} ({})", crate::VERSION, build.channel),
    }
}

/// What the shell says about itself before its first prompt.
fn banner(config: &RuntimeConfig, model: &str) -> String {
    let mut out = format!(
        "{} -- unofficial, experimental Rust port of fx\n",
        version_line()
    );
    let _ = writeln!(
        out,
        "[shell] model={model} permission_mode={} sandbox={SANDBOX_LABEL}",
        config.permission_mode.label()
    );
    let _ = writeln!(out, "[shell] workspace={}", config.workspace_root.display());
    let _ = writeln!(
        out,
        "[shell] type a prompt, or /help for the {} commands; Ctrl-D leaves",
        SLASH_COMMANDS.len()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_slash_command_parses_back_to_itself() {
        for name in SLASH_COMMANDS {
            let command = Slash::parse(name).unwrap_or_else(|| panic!("{name} does not parse"));
            assert_eq!(command.name(), *name);
        }
    }

    #[test]
    fn the_shell_advertises_exactly_the_registrys_commands() {
        // The count is a product decision, not an accident: the shell is
        // deliberately smaller than upstream's ~40-command palette
        // (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:463-499`).
        // Ten since `/copy`, `/stats` and `/status`, eleven since `/reset` --
        // upstream's third name for the one reset -- thirteen since
        // `/permissions` and `/statusline`, fourteen since `/sound`, sixteen
        // since `/resume` and `/rename`, seventeen since `/undo`, nineteen
        // since `/usage` and `/credits`, twenty since `/trace`, twenty-one
        // since `/settings`, twenty-four since `/image`, `/images` and
        // `/paste`, twenty-five since `/allowlist`, twenty-six since
        // `/workspace`, twenty-seven since `/compact`, and twenty-nine since
        // `/fast` and `/ultrafast`, thirty since `/mcp`, thirty-one since
        // `/skills`, and thirty-two since `/alias`; `/provider` took `/setup`'s
        // place rather than adding one, and `/models` stays deferred.
        assert_eq!(SLASH_COMMANDS.len(), 32);
        let mut sorted = SLASH_COMMANDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), SLASH_COMMANDS.len(), "a name is repeated");
    }

    #[test]
    fn a_blank_line_is_not_a_prompt() {
        assert_eq!(classify(""), Submitted::Blank);
        assert_eq!(classify("   \t "), Submitted::Blank);
        assert_eq!(classify("\n"), Submitted::Blank);
    }

    #[test]
    fn a_leading_slash_is_the_only_thing_that_makes_a_command() {
        assert_eq!(
            classify("what does a/b /help mean"),
            Submitted::Prompt("what does a/b /help mean".to_string())
        );
        assert_eq!(
            classify("  /quit  "),
            Submitted::Command {
                command: Slash::Quit,
                argument: String::new()
            }
        );
    }

    #[test]
    fn a_command_keeps_the_rest_of_its_line_as_one_argument() {
        assert_eq!(
            classify("/model acme/model-9"),
            Submitted::Command {
                command: Slash::Model,
                argument: "acme/model-9".to_string()
            }
        );
        assert_eq!(
            classify("/model  one two "),
            Submitted::Command {
                command: Slash::Model,
                argument: "one two".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_command_is_named_exactly_and_matched_exactly() {
        assert_eq!(
            classify("/nonesuch now"),
            Submitted::UnknownCommand {
                token: "/nonesuch".to_string()
            }
        );
        // No case folding and no prefix guessing: a refusal has to be the same
        // sentence every time. `/exit` is **not** in this list any more: it is
        // a registry alias for `/quit` and is answered rather than refused
        // (`exit_is_an_alias_for_quit_and_not_a_canonical_name`).
        for near_miss in ["/HELP", "/hel", "/helpme", "/exitx"] {
            assert_eq!(
                classify(near_miss),
                Submitted::UnknownCommand {
                    token: near_miss.to_string()
                },
                "{near_miss}"
            );
        }
    }

    #[test]
    fn the_refusal_is_a_pure_function_of_the_name() {
        assert_eq!(
            unknown_command_message("/nonesuch"),
            unknown_command_message("/nonesuch")
        );
        // Exact, and derived: the sentence quotes the registry's own count, so
        // this expectation moves with the palette instead of pinning a word
        // that goes stale the next time a command is added.
        assert_eq!(
            unknown_command_message("/nonesuch"),
            format!(
                "xfx: `/nonesuch` is not an xfx command; /help lists its {} commands",
                SLASH_COMMANDS.len()
            )
        );
    }

    #[test]
    fn a_refusal_never_quotes_back_a_control_character() {
        // A line beginning `/` is echoed by the terminal *and* quoted by xfx.
        // The echo is the terminal's own business; the quote is xfx's, and it
        // must not be a way to make xfx clear the screen, retitle the window,
        // or move the cursor on the user's behalf.
        let hostile = "/\u{1b}[2J\u{1b}]0;pwned\u{7}\u{1b}[H";
        let message = unknown_command_message(hostile);
        assert!(!message.contains('\u{1b}'), "{message:?}");
        assert!(!message.chars().any(char::is_control), "{message:?}");
        assert_eq!(message.lines().count(), 1, "{message:?}");
        assert!(message.contains("/help"), "{message:?}");
    }

    #[test]
    fn a_refusal_is_bounded_however_long_the_mistake_was() {
        let message = unknown_command_message(&format!("/{}", "x".repeat(100_000)));
        assert!(
            message.len() < MAX_QUOTED_COMMAND_BYTES + 128,
            "{message:?}"
        );
        assert!(message.contains('…'), "{message:?}");
        // The guidance is the part worth reading, so it survives the clip.
        assert!(
            message.ends_with(&format!(
                "/help lists its {} commands",
                SLASH_COMMANDS.len()
            )),
            "{message:?}"
        );
    }

    #[test]
    fn a_refusal_keeps_a_multibyte_typo_readable() {
        let message = unknown_command_message("/설명");
        assert!(message.contains("/설명"), "{message}");
        // Clipping is on a character boundary, so a long multibyte token can
        // never produce invalid text.
        let long = unknown_command_message(&format!("/{}", "설".repeat(1000)));
        assert!(long.contains('…'), "{long}");
    }

    #[test]
    fn help_lists_every_command_and_nothing_else() {
        let help = help_text();
        for name in SLASH_COMMANDS {
            assert!(help.contains(name), "help omits {name}: {help}");
        }
        // One line per command, plus the title and the closing note.
        assert_eq!(help.lines().count(), SLASH_COMMANDS.len() + 2, "{help}");
    }

    #[test]
    fn help_advertises_no_deferred_upstream_command() {
        let help = help_text();
        for deferred in ["/login", "/logout", "/models", "/background"] {
            assert!(!help.contains(deferred), "help advertises {deferred}");
        }
    }

    #[test]
    fn the_refusals_say_what_to_do_instead() {
        assert!(NO_TERMINAL.contains("xfx ask"));
        assert!(NO_STORE.contains("--no-save"));
    }

    #[test]
    fn an_idle_interrupt_becomes_an_exit_only_on_the_second_one() {
        let interrupts = Interrupts::new(CancelToken::new());
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
        // A submitted line puts the count back, which is what keeps two
        // interrupts an hour apart from ending the shell.
        interrupts.line_submitted();
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
    }

    #[test]
    fn beginning_a_turn_clears_the_previous_turns_cancellation() {
        let cancel = CancelToken::new();
        let interrupts = Interrupts::new(cancel.clone());
        cancel.cancel();
        interrupts.begin_turn();
        assert!(!cancel.is_cancelled(), "the next turn started cancelled");
        assert!(matches!(
            *interrupts.lock(),
            Activity::Running { cancelled: false }
        ));
        interrupts.end_turn();
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
    }

    #[test]
    fn the_notice_a_cancelled_turn_writes_promises_the_exit_this_table_honours() {
        // `Interrupts::signalled` writes `app::INTERRUPT_NOTICE` on the arm
        // below, so the sentence is a claim about this state machine. The claim
        // it may make is the one the next arm keeps -- a press while the turn is
        // still `Running { cancelled: true }` exits 130 -- and not the one
        // `end_turn` takes away: from `Idle` the next press offers a fresh
        // prompt line, and it takes two of them to leave.
        //
        // Driven through `signalled` rather than by writing the state, so the
        // arm that writes the notice is the arm that is measured. The exiting
        // arm is not called: it is `process::exit`, which a test cannot survive,
        // so what is asserted is the state that reaches it.
        let cancel = CancelToken::new();
        let interrupts = Interrupts::new(cancel.clone());
        interrupts.begin_turn();

        interrupts.signalled();

        assert!(cancel.is_cancelled(), "the turn was not asked to stop");
        assert!(
            matches!(*interrupts.lock(), Activity::Running { cancelled: true }),
            "the notice was written from a state whose next press does not exit"
        );
        interrupts.end_turn();
        assert!(
            matches!(*interrupts.lock(), Activity::Idle { consecutive: 0 }),
            "a turn that ended left the exiting arm armed"
        );
        assert!(
            crate::app::INTERRUPT_NOTICE.contains("before it stops"),
            "the notice promises an exit this table does not honour once the \
             turn has ended: {}",
            crate::app::INTERRUPT_NOTICE
        );
    }

    #[test]
    fn the_canonical_list_and_the_registry_are_one_order() {
        assert_eq!(SLASH_REGISTRY.len(), SLASH_COMMANDS.len());
        for (spec, name) in SLASH_REGISTRY.iter().zip(SLASH_COMMANDS) {
            assert_eq!(spec.name, *name);
            assert_eq!(spec.command.name(), *name);
        }
    }

    #[test]
    fn every_canonical_name_has_exactly_one_spec() {
        for name in SLASH_COMMANDS {
            let command = Slash::parse(name).expect("a canonical name parses");
            let found: Vec<&SlashSpec> = SLASH_REGISTRY
                .iter()
                .filter(|spec| spec.command == command)
                .collect();
            assert_eq!(found.len(), 1, "{name}");
            assert_eq!(slash_spec(command).name, *name);
        }
    }

    #[test]
    fn exit_is_an_alias_for_quit_and_not_a_canonical_name() {
        assert_eq!(
            classify("/exit"),
            Submitted::Command {
                command: Slash::Quit,
                argument: String::new()
            }
        );
        assert!(!SLASH_COMMANDS.contains(&"/exit"));
        assert_eq!(slash_spec(Slash::Quit).aliases, &["/exit"]);
        assert_eq!(Slash::Quit.name(), "/quit");
    }

    #[test]
    fn help_distinguishes_an_alias_without_advertising_another_command() {
        let help = help_text();
        assert!(help.contains("/exit"), "{help}");
        assert_eq!(help.lines().count(), SLASH_COMMANDS.len() + 2, "{help}");
        let named: Vec<&str> = help
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|word| word.starts_with('/'))
            .collect();
        assert_eq!(named, SLASH_COMMANDS.to_vec(), "{help}");
    }

    #[test]
    fn only_a_command_that_takes_one_is_declared_to_have_an_argument() {
        // The five that mean something by the rest of the line: `/model <id>`
        // picks among what a provider offers, `/provider <provider>` (or
        // `/setup <provider>`) picks the provider -- upstream's row has args
        // too, which is what puts the space after it when it is completed
        // (`commands.zig:472`) -- `/permissions <mode|reset>` picks the
        // authority, `/allowlist [view|add|remove|reset ...]` reads or edits
        // the persistent allow rules, `/statusline <item>` flips a footer
        // segment, `/sound [on|off]` sets the switch a bare `/sound` toggles,
        // `/rename <title>` names the session, `/image <path>` attaches a
        // file, `/images [clear]` lists or drops what is attached,
        // `/workspace [list|add PATH|remove PATH|clear]` keeps the added
        // directories, and `/ultrafast [on|off|status]` asks for Ultra mode.
        // `/settings` is declared to take one as upstream's row is
        // (`commands.zig:488`), so completing it leaves the caret past a
        // space; every argument it is given is its usage line, since
        // upstream's only form names a setting xfx does not have. `/alias` is
        // declared to take one as upstream's row is (`commands.zig:489`), and
        // answers every form with the same sentence. Every other name is the
        // whole command -- `/fast` among them, a toggle (`commands.zig:492`).
        for spec in SLASH_REGISTRY {
            let takes_one = matches!(
                spec.name,
                "/model"
                    | "/provider"
                    | "/permissions"
                    | "/allowlist"
                    | "/statusline"
                    | "/sound"
                    | "/rename"
                    | "/ultrafast"
                    | "/image"
                    | "/images"
                    | "/settings"
                    | "/workspace"
                    | "/mcp"
                    | "/skills"
                    | "/alias"
            );
            assert_eq!(spec.has_args, takes_one, "{}", spec.name);
        }
    }

    #[test]
    fn provider_is_canonical_and_setup_is_its_alias() {
        // Upstream's row (`commands.zig:472`): `/provider`, with `/setup` as
        // the alias. `/setup` was canonical here before; it is still answered,
        // by the same command, and the count of canonical names did not move.
        assert!(SLASH_COMMANDS.contains(&"/provider"), "{SLASH_COMMANDS:?}");
        assert!(!SLASH_COMMANDS.contains(&"/setup"), "{SLASH_COMMANDS:?}");
        assert_eq!(Slash::parse("/provider"), Some(Slash::Setup));
        assert_eq!(Slash::parse("/setup"), Some(Slash::Setup));
        assert_eq!(Slash::Setup.name(), "/provider");
        let spec = slash_spec(Slash::Setup);
        assert_eq!(
            (spec.aliases, spec.summary, spec.category, spec.has_args),
            (
                &["/setup"][..],
                "choose the model provider and how it signs in",
                "Account",
                true
            )
        );
        assert_eq!(
            classify("/setup llmux"),
            Submitted::Command {
                command: Slash::Setup,
                argument: "llmux".to_string()
            }
        );
        // `/help` says the alias on the command's own line.
        assert!(
            help_text()
                .lines()
                .any(|line| line.trim_start().starts_with("/provider")
                    && line.ends_with("(also /setup)")),
            "{}",
            help_text()
        );
    }

    #[test]
    fn stats_and_status_are_upstreams_rows_and_take_nothing() {
        // `commands.zig:473,475`: the category and the description the menu
        // shows, and neither row `accepts_payload` -- so the TUI refuses
        // `/stats extra` as an unknown command (`super::tui`'s router).
        let stats = slash_spec(Slash::Stats);
        assert_eq!(
            (stats.name, stats.summary, stats.category, stats.has_args),
            ("/stats", "show token and turn statistics", "Account", false)
        );
        let status = slash_spec(Slash::Status);
        assert_eq!(
            (
                status.name,
                status.summary,
                status.category,
                status.has_args
            ),
            ("/status", "show runtime configuration", "General", false)
        );
        // Upstream's order: `/provider`, `/stats`, `/usage`, `/status`, then
        // `/model` (`commands.zig:472-478`).
        let order: Vec<&str> = SLASH_COMMANDS.to_vec();
        let at = |name: &str| order.iter().position(|n| *n == name).expect(name);
        assert!(at("/provider") < at("/stats") && at("/stats") < at("/usage"));
        assert!(at("/usage") < at("/status"));
        assert!(at("/status") < at("/model"));
        // And `/credits`, then `/fast` and `/ultrafast`, then `/statusline`
        // (`commands.zig:490-494`).
        assert!(at("/credits") < at("/fast") && at("/fast") < at("/ultrafast"));
        assert!(at("/ultrafast") < at("/statusline"));
        assert!(help_text().contains("/status"), "{}", help_text());
    }

    #[test]
    fn status_reports_the_model_in_force_and_the_conversation_it_is_given() {
        // The `xfx status` facts, about this shell rather than the store's last
        // session: the model the selector holds, and no conversation as none.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), Default::default()),
            workspace.path(),
        )
        .expect("load a configuration");
        let snapshot = status_snapshot(&config, "acme/model-9", None);
        assert_eq!(snapshot.model, "acme/model-9");
        assert_ne!(config.model, "acme/model-9", "the case proves nothing");
        assert_eq!(snapshot.history_turns, 0);
        assert_eq!(snapshot.session_permission_grants, 0);
        assert_eq!(
            snapshot.workspace,
            config.workspace_root.display().to_string()
        );
        assert!(
            snapshot
                .render_text()
                .starts_with("[status] model=acme/model-9\n"),
            "{}",
            snapshot.render_text()
        );
    }

    #[test]
    fn stats_on_the_line_shell_points_at_the_counters_rather_than_inventing_them() {
        // This shell writes lines and draws no frames, so the honest answer is
        // where the counters are -- not a row of zeros about frames that never
        // existed.
        assert!(!STATS_ON_THE_LINE_SHELL.contains("ansi_bytes="));
        assert!(STATS_ON_THE_LINE_SHELL.contains("XFX_TUI=0"));
        assert!(STATS_ON_THE_LINE_SHELL.starts_with("xfx: /stats"));
    }

    #[test]
    fn the_media_commands_are_upstreams_rows_and_say_where_they_work() {
        // `commands.zig:476-477`, `:491`: the names, the `/img` alias, the
        // sentences and the Media tab.
        for (command, name, aliases, summary, has_args) in [
            (
                Slash::Image,
                "/image",
                &["/img"][..],
                "attach an image by path",
                true,
            ),
            (
                Slash::Images,
                "/images",
                &[],
                "manage pending image attachments",
                true,
            ),
            (
                Slash::Paste,
                "/paste",
                &[],
                "attach an image from the clipboard when supported",
                false,
            ),
        ] {
            let spec = slash_spec(command);
            assert_eq!(spec.name, name);
            assert_eq!(spec.aliases, aliases);
            assert_eq!(spec.summary, summary);
            assert_eq!(spec.category, "Media");
            assert_eq!(spec.has_args, has_args);
        }
        assert_eq!(
            classify("/img shot.png"),
            Submitted::Command {
                command: Slash::Image,
                argument: "shot.png".to_string()
            }
        );
        // This shell has no draft to attach to, and says where there is one.
        assert!(IMAGES_ON_THE_LINE_SHELL.contains("XFX_TUI=0"));
        assert!(IMAGES_ON_THE_LINE_SHELL.starts_with("xfx: /image"));
    }

    #[test]
    fn usage_and_credits_are_upstreams_account_rows_with_their_aliases() {
        // `commands.zig:474,490`: name, alias, sentence (xfx's name in place
        // of upstream's in `/usage`'s) and category, and neither row
        // `accepts_payload` -- `/credits now` is not `/credits`.
        let usage = slash_spec(Slash::Usage);
        assert_eq!(
            (
                usage.name,
                usage.aliases,
                usage.summary,
                usage.category,
                usage.has_args
            ),
            (
                "/usage",
                &["/cost"][..],
                "show local xfx tokens, models, and spend",
                "Account",
                false
            )
        );
        let credits = slash_spec(Slash::Credits);
        assert_eq!(
            (
                credits.name,
                credits.aliases,
                credits.summary,
                credits.category,
                credits.has_args
            ),
            (
                "/credits",
                &["/balance"][..],
                "show gateway credits balance",
                "Account",
                false
            )
        );
        for (alias, command) in [("/cost", Slash::Usage), ("/balance", Slash::Credits)] {
            assert_eq!(
                classify(alias),
                Submitted::Command {
                    command,
                    argument: String::new()
                }
            );
            assert!(!SLASH_COMMANDS.contains(&alias), "{alias}");
        }
        // Upstream's order: `/copy` before `/credits` before `/statusline`
        // (`commands.zig:484-494`).
        let at = |name: &str| SLASH_COMMANDS.iter().position(|n| *n == name).expect(name);
        assert!(at("/copy") < at("/credits") && at("/credits") < at("/statusline"));
        let help = help_text();
        assert!(
            help.contains("(also /cost)") && help.contains("(also /balance)"),
            "{help}"
        );
    }

    #[test]
    fn alias_is_upstreams_extensions_row_and_takes_the_rest_of_the_line() {
        // `commands.zig:489`: the sentence the menu shows, the Extensions tab,
        // no alias of its own, and `has_args` with `accepts_payload` -- so a
        // name and a command after it are the command's argument rather than
        // an unknown command.
        let alias = slash_spec(Slash::Alias);
        assert_eq!(
            (
                alias.name,
                alias.aliases,
                alias.summary,
                alias.category,
                alias.has_args
            ),
            (
                "/alias",
                &[][..],
                "show alias availability",
                "Extensions",
                true
            )
        );
        for (line, argument) in [
            ("/alias", ""),
            ("/alias build", "build"),
            ("/alias build zig build", "build zig build"),
        ] {
            assert_eq!(
                classify(line),
                Submitted::Command {
                    command: Slash::Alias,
                    argument: argument.to_string()
                },
                "{line:?}"
            );
        }
        // Upstream's order: `/settings`, `/alias`, `/credits`
        // (`commands.zig:488-490`).
        let at = |name: &str| SLASH_COMMANDS.iter().position(|n| *n == name).expect(name);
        assert!(at("/settings") + 1 == at("/alias") && at("/alias") + 1 == at("/credits"));
        // Upstream's sentence under upstream's topic, the same words on both
        // surfaces (`app_commands.zig:2046-2048`).
        assert_eq!(ALIAS_TOPIC, "aliases");
        assert_eq!(
            ALIASES_NOT_CONFIGURABLE,
            "Aliases are not yet configurable."
        );
        assert!(help_text()
            .lines()
            .any(|line| line == "  /alias    show alias availability"));
    }

    #[test]
    fn usage_on_the_line_shell_points_at_the_dashboard() {
        assert!(USAGE_ON_THE_LINE_SHELL.starts_with("xfx: /usage"));
        assert!(USAGE_ON_THE_LINE_SHELL.contains("XFX_TUI=0"));
    }

    #[test]
    fn sound_is_upstreams_appearance_row_and_takes_its_level() {
        // `commands.zig:495`: the description the menu shows and the category
        // it is filed under, and `accepts_payload` -- `/sound on` is the
        // command with an argument, not an unknown one.
        let sound = slash_spec(Slash::Sound);
        assert_eq!(
            (sound.name, sound.summary, sound.category, sound.has_args),
            (
                "/sound",
                "toggle sounds and terminal bells",
                "Appearance",
                true
            )
        );
        // Upstream's order puts it after `/copy` and before `/version`
        // (`commands.zig:484-498`).
        let at = |name: &str| SLASH_COMMANDS.iter().position(|n| *n == name).expect(name);
        assert!(at("/copy") < at("/sound") && at("/sound") < at("/version"));
        assert_eq!(
            classify("/sound on"),
            Submitted::Command {
                command: Slash::Sound,
                argument: "on".to_string()
            }
        );
        assert!(help_text().contains("/sound"), "{}", help_text());
    }

    #[test]
    fn sound_on_the_line_shell_saves_the_switch_and_says_whose_bell_it_is() {
        let dir = tempfile::tempdir().expect("home");
        let path = dir.path().join(".xfx").join("settings.json");
        let on = apply_sound("", crate::config::Notifications::both(false), Some(&path))
            .expect("toggle");
        assert_eq!(on, crate::config::Notifications::both(true));
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("saved")).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"notifications": {"turn_end": true, "attention_required": true}})
        );
        let off = apply_sound("off", on, Some(&path)).expect("set");
        assert_eq!(off, crate::config::Notifications::both(false));
        // The usage line changes nothing and writes nothing.
        let before = std::fs::read(&path).expect("read");
        assert_eq!(apply_sound("max", off, Some(&path)).expect("usage"), off);
        assert_eq!(std::fs::read(&path).expect("read"), before);
        assert!(SOUND_ON_THE_LINE_SHELL.contains("rings none"));
    }

    #[test]
    fn the_standalone_catalog_command_is_still_deferred() {
        // `/models` is an upstream command xfx does not answer: the catalog
        // browser is reached by a bare `/model`, so advertising it would be
        // advertising a surface that is not there. (`/provider` used to be
        // listed here; it is answered now, as upstream's row with `/setup` as
        // its alias -- `provider_is_canonical_and_setup_is_its_alias`.)
        let absent = "/models";
        assert!(!SLASH_COMMANDS.contains(&absent), "{absent} is canonical");
        assert_eq!(Slash::parse(absent), None, "{absent} parses");
        assert!(!help_text().contains(absent), "help advertises {absent}");
    }

    #[test]
    fn the_unknown_command_refusal_counts_the_registry_rather_than_a_literal() {
        // The refusal used to say "the six it has" as a literal, which is a
        // second declaration of the palette's size and therefore a second thing
        // that can drift out of step with it. It is derived now: a name added
        // to the registry changes this sentence without anybody editing it.
        let refusal = unknown_command_message("/nonesuch");
        assert!(
            refusal.contains(&SLASH_COMMANDS.len().to_string()),
            "the refusal does not carry the registry's own count: {refusal}"
        );
        assert!(
            !refusal.contains("six"),
            "the refusal still spells a count as a literal word: {refusal}"
        );
        // Still one bounded, sanitized line: deriving the count must not have
        // moved the guidance off the end of it.
        assert!(refusal.ends_with("commands"), "{refusal}");
    }

    /// A configuration over `root/home` and `root/work`, both kept by the
    /// caller's temporary directory.
    fn config_under(root: &std::path::Path) -> RuntimeConfig {
        let home = root.join("home");
        let workspace = root.join("work");
        std::fs::create_dir_all(&home).expect("a home");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        RuntimeConfig::load_with(
            &Environment::new(Some(home), std::collections::BTreeMap::new()),
            &workspace,
        )
        .expect("load a configuration")
    }

    /// `/workspace <argument>` on this shell: what it printed on each stream.
    fn workspace_said_on(argument: &str, config: &mut RuntimeConfig) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        workspace_on_the_line(argument, config, None, &mut out, &mut err).expect("write");
        (
            String::from_utf8(out).expect("utf-8"),
            String::from_utf8(err).expect("utf-8"),
        )
    }

    #[test]
    fn the_line_shell_lists_adds_and_keeps_a_workspace_directory() {
        let root = tempfile::tempdir().expect("a root");
        let mut config = config_under(root.path());
        let primary = config.workspace_root.clone();
        let extra = primary.parent().expect("a parent").join("extra");
        std::fs::create_dir(&extra).expect("extra");

        let (out, err) = workspace_said_on("", &mut config);
        assert_eq!(err, "");
        assert_eq!(
            out,
            format!(
                "[shell] workspace: primary={}\n\
                 [shell]   saved_suppressed=false limit=16\n\
                 [shell]   additional directories: (none)\n",
                primary.display()
            )
        );

        let (out, err) = workspace_said_on("add ../extra", &mut config);
        assert_eq!(err, "");
        assert!(
            out.contains(
                "\n[shell]   add ../extra saved_changed=true runtime_changed=true \
                 launch_flag_can_restore=false\n"
            ),
            "{out}"
        );
        assert!(
            out.ends_with(&format!(
                "[shell]    - {} saved=true command_line=false available=true active=true\n",
                extra.display()
            )),
            "{out}"
        );
        // In force for the next conversation this shell opens, and for the
        // next launch.
        assert_eq!(
            conversation_scope(&config)
                .expect("a scope")
                .additional_roots(),
            std::slice::from_ref(&extra)
        );
        let reloaded = config_under(root.path());
        assert_eq!(
            reloaded.additional_directories,
            [extra.to_string_lossy().into_owned()]
        );
        assert_eq!(
            conversation_scope(&reloaded)
                .expect("a scope")
                .additional_roots(),
            [extra]
        );

        let (out, err) = workspace_said_on("clear", &mut config);
        assert_eq!(err, "");
        assert!(
            out.ends_with("[shell]   additional directories: (none)\n"),
            "{out}"
        );
        assert!(config.additional_directories.is_empty());
    }

    #[test]
    fn the_line_shell_refuses_a_workspace_change_in_upstreams_words() {
        let root = tempfile::tempdir().expect("a root");
        let mut config = config_under(root.path());
        let (out, err) = workspace_said_on("bogus", &mut config);
        assert_eq!(out, "");
        assert_eq!(err, "xfx: usage: /workspace [add PATH|remove PATH|clear]\n");
        let (out, err) = workspace_said_on("add ../missing", &mut config);
        assert_eq!(out, "");
        assert_eq!(
            err,
            "xfx: workspace: Workspace update rejected: directory does not exist\n"
        );
        assert!(config.additional_directories.is_empty());
    }

    /// `/statusline <argument>` on this shell: what it printed on each stream.
    fn statusline_said(argument: &str, config: &mut RuntimeConfig) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        statusline_on_the_line(argument, config, &mut out, &mut err).expect("write");
        (
            String::from_utf8(out).expect("utf-8"),
            String::from_utf8(err).expect("utf-8"),
        )
    }

    #[test]
    fn the_line_shell_flips_and_keeps_a_statusline_item_in_the_tuis_words() {
        let root = tempfile::tempdir().expect("a root");
        let mut config = config_under(root.path());
        let (out, err) = statusline_said("workspace", &mut config);
        assert_eq!(
            out,
            "[shell] statusline: saved to user settings (scope=user)\n\
             [shell] statusline: workspace: on\n"
        );
        assert_eq!(err, "");
        assert!(config.statusline.workspace);
        // What it kept is what the next launch reads.
        let reloaded = config_under(root.path());
        assert!(reloaded.statusline.workspace);
        assert!(!reloaded.statusline.context);

        let (out, _) = statusline_said("workspace", &mut config);
        assert!(
            out.ends_with("[shell] statusline: workspace: off\n"),
            "{out}"
        );
        assert!(!config_under(root.path()).statusline.workspace);
    }

    #[test]
    fn a_bare_statusline_on_the_line_shell_says_each_item_and_how_to_change_one() {
        let root = tempfile::tempdir().expect("a root");
        let mut config = config_under(root.path());
        config.statusline.session = true;
        let (out, err) = statusline_said("", &mut config);
        assert_eq!(
            out,
            "[shell] statusline: context: off, session: on, workspace: off\n\
             [shell] usage: /statusline [context|session|workspace]\n"
        );
        assert_eq!(err, "");
        assert!(
            !root.path().join("home").join(".xfx").exists(),
            "asking wrote the profile"
        );
    }

    #[test]
    fn the_line_shell_refuses_an_item_it_does_not_have_and_says_a_failed_write() {
        let root = tempfile::tempdir().expect("a root");
        let mut config = config_under(root.path());
        let (out, err) = statusline_said("Context", &mut config);
        assert_eq!(out, "");
        assert_eq!(err, "xfx: usage: /statusline [context|session|workspace]\n");
        assert_eq!(config.statusline, crate::config::StatusLine::default());

        // No home: in force for this process, and said not to be kept.
        config.user_settings_path = None;
        let (out, err) = statusline_said("context", &mut config);
        assert_eq!(out, "[shell] statusline: context: on\n");
        assert_eq!(
            err,
            "xfx: statusline: active for this process but not saved to user settings \
             (no home directory is set)\n"
        );
        assert!(config.statusline.context);
    }

    /// `/trace` on this shell into `dir`, copying with `tool`: what it printed
    /// on each stream.
    fn trace_said(
        config: &RuntimeConfig,
        conversation: Option<&Conversation>,
        store: &SessionStore,
        dir: &std::path::Path,
        tool: &crate::clipboard::fake::FakeTool,
    ) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let target = TraceTarget {
            dir: dir.to_path_buf(),
            clipboard: tool.clipboard(),
        };
        trace_on_the_line(
            config,
            &config.model,
            conversation,
            store,
            &target,
            &mut out,
            &mut err,
        )
        .expect("write");
        (
            String::from_utf8(out).expect("utf-8"),
            String::from_utf8(err).expect("utf-8"),
        )
    }

    /// The one report written under `dir`.
    fn one_report(dir: &std::path::Path) -> (std::path::PathBuf, String) {
        let written: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .expect("the report directory")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        assert_eq!(written.len(), 1, "{written:?}");
        let report = std::fs::read_to_string(&written[0]).expect("the report");
        (written[0].clone(), report)
    }

    #[test]
    fn the_line_shell_writes_a_report_about_its_own_conversation_and_says_so() {
        let root = tempfile::tempdir().expect("a root");
        let config = config_under(root.path());
        let store = SessionStore::open(&root.path().join("home").join(".xfx")).expect("a store");
        let dir = tempfile::tempdir().expect("a report directory");
        let tool = crate::clipboard::fake::FakeTool::keeping();

        // Before the first prompt there is no session to describe.
        let (out, err) = trace_said(&config, None, &store, dir.path(), &tool);
        let (path, report) = one_report(dir.path());
        let expected = if cfg!(target_os = "macos") {
            crate::trace::Disposition::Copied(path.clone())
        } else {
            crate::trace::Disposition::Saved(path.clone())
        };
        assert_eq!(out, format!("[shell] {}\n", expected.sentence()));
        assert_eq!(err, "");
        assert!(report.contains("\nsession: none (xfx opens one at the first prompt)\n"));
        if cfg!(target_os = "macos") {
            assert_eq!(tool.copied_file().as_deref(), path.to_str());
        }
        std::fs::remove_file(&path).expect("clear the directory");

        // With one, the live conversation's own state.
        let mut conversation = open_conversation(
            &store,
            &config,
            &config.model,
            crate::app::permission_session(config.permission_mode, LiveRules::default()),
            None,
            &CancelToken::new(),
        )
        .expect("a conversation");
        conversation.recorder.commit(SessionEvent::UserMessage {
            text: "what is in here?".to_string(),
        });
        let _ = trace_said(&config, Some(&conversation), &store, dir.path(), &tool);
        let (_, report) = one_report(dir.path());
        for line in [
            format!("session_id: {}", conversation.recorder.id()),
            "history_turns: 1".to_string(),
            "    what is in here?".to_string(),
        ] {
            assert!(report.lines().any(|row| row == line), "{line}\n{report}");
        }
        assert!(
            !report.contains("## Transcript Timeline"),
            "this shell has no screen to describe: {report}"
        );
    }

    #[test]
    fn the_line_shell_says_a_failed_copy_or_write_on_the_diagnostic_stream() {
        let root = tempfile::tempdir().expect("a root");
        let config = config_under(root.path());
        let store = SessionStore::read_only(&root.path().join("home").join(".xfx"));
        let tool = crate::clipboard::fake::FakeTool::keeping();
        tool.fail();
        let dir = tempfile::tempdir().expect("a report directory");
        let (out, err) = trace_said(&config, None, &store, dir.path(), &tool);
        let (path, _) = one_report(dir.path());
        if cfg!(target_os = "macos") {
            assert_eq!(out, "");
            assert_eq!(
                err,
                format!(
                    "xfx: {}\n",
                    crate::trace::Disposition::CopyFailed(path).sentence()
                )
            );
        } else {
            assert_eq!(
                out,
                format!(
                    "[shell] {}\n",
                    crate::trace::Disposition::Saved(path).sentence()
                )
            );
        }
        let (out, err) = trace_said(
            &config,
            None,
            &store,
            std::path::Path::new("/nonexistent/xfx-trace-dir"),
            &tool,
        );
        assert_eq!(
            (out.as_str(), err.as_str()),
            ("", "xfx: Could not create trace.\n")
        );
    }

    /// `/allowlist <argument>` on this shell: what it printed on each stream.
    fn allowlist_said(
        argument: &str,
        config: &RuntimeConfig,
        rules: &LiveRules,
    ) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        allowlist_on_the_line(argument, config, rules, &mut out, &mut err).expect("write");
        (
            String::from_utf8(out).expect("utf-8"),
            String::from_utf8(err).expect("utf-8"),
        )
    }

    #[test]
    fn the_line_shell_says_allowlist_answers_as_lines_and_puts_the_table_in_force() {
        use crate::permission::{ConfiguredRule, RuleAction};
        let root = tempfile::tempdir().expect("a root");
        let config = config_under(root.path());
        let rules = LiveRules::default();
        let (out, err) = allowlist_said("add command \"git *\"", &config, &rules);
        assert_eq!(out, "[allowlist] added command: \"git *\" (scope=local)\n");
        assert_eq!(err, "");
        assert_eq!(
            rules.current().rules(),
            &[ConfiguredRule::new("bash", "git *", RuleAction::Allow)]
        );
        let (out, _) = allowlist_said("view local", &config, &rules);
        assert_eq!(
            out,
            "[allowlist] local persistent allow rules:\n  commands:\n    git *\n"
        );
        let (out, err) = allowlist_said("reset", &config, &rules);
        assert_eq!(out, "");
        assert_eq!(
            err,
            "xfx: usage: /allowlist reset [commands|tools|urls|web-fetch-domains|all]\n"
        );
        // A pattern is the person's own text: a control byte in it is not
        // handed to the terminal.
        let (out, _) = allowlist_said("add command \"a\u{1b}[2Jb\"", &config, &rules);
        assert!(!out.contains('\u{1b}'), "{out:?}");
    }
}
