# W3 receipt: /trace

Scenarios `scripts/fxcap/scenarios/trace.cap` (turn `trace-turn.json`) and
`scripts/fxcap/scenarios/trace-nowhere.cap`, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  trace         40x120 <dir>
                                  scripts/fxcap/capture.sh xfx trace         40x120 <dir>
FX_BIN=...                        scripts/fxcap/capture.sh fx  trace-nowhere 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx trace-nowhere 40x120 <dir>
```

`trace` types `/trace` before any turn, then `/trace now`, then one prompt
whose answer is `Traced answer.`, then `/trace`, then `/trace` again with the
clipboard set to refuse. `trace-nowhere` launches with
`TMPDIR=/nonexistent/fxcap-trace-dir` and types `/trace` once.

## Harness changes (`scripts/fxcap/capture.sh`)

All additive; the shims, `FX_NO_OPEN_BROWSER=1` and the fx sound guard are
unchanged.

- xfx gets `TMPDIR=$run/xfxtmp`, the mirror of fx's `$run/fxtmp`, so its report
  stays inside the run instead of landing in `/tmp`.
- The `osascript` shim still writes nothing to the clipboard and still exits 0.
  It now appends its last argument (the path a file copy hands it) to
  `$run/clipboard-file`. Under `clipfail on` it exits 1 and keeps nothing,
  as the `pbcopy` shim does. It is run once before the session starts, as
  `pbcopy` already was: in a run without that warm-up, xfx's first copy had
  not finished when the screen was taken 2 s later (macOS assesses a new
  executable on its first run).
- `clipfile <label>` moves what the `osascript` shim kept to
  `<bin>-<scenario>-<size>-<label>.clipfile`. `reports <label>` copies every
  `*-trace-*.md` in the binary's own TMPDIR to `<label>-<n>.md`, oldest first,
  and prints the mode each was written with. Both write the run's directory as
  `<scratch>`.

Nothing reached the real clipboard: `osascript` resolved to the shim on both
binaries (the `.clipfile`s are the shim's record). The fx run's chime
directories were intact (exit 0, no `SOUND GUARD BREACHED`).

The xfx binary is `target/debug/xfx` built from the working tree of this
commit. Its banner shows `cafab50` because that is the base commit. The first
xfx run after that build lost the `/trace` typed 2 s after launch (no report,
no notice; the TUI was not yet reading). That run was discarded. These files
are from the next run.

Sanitized: the run directory `/private/tmp/claude-501/fxcap.XXXXXX` is written
as `<scratch>` in every file here. It appears in the `copy-fail` screens (the
saved path) and in the reports and `.clipfile`s.

## Per snap

| snap | fx | xfx | matches |
|---|---|---|---|
| `trace-empty` | `* Trace copied to clipboard. Review and redact it before sharing.` | same | Row 3 is byte for byte the same, SGR included (`ESC[38;5;250m* Trace copied to clipboard. Review and redact it before sharing.ESC[39m`, no topic). No echo of `/trace`. The `.clipfile` on each names that binary's report: `<scratch>/fxtmp/fx-trace-…md` and `<scratch>/xfxtmp/xfx-trace-…md`. |
| `trace-now` | `✗ command: Unknown command. Try /help.` | same | Row 5 is byte for byte the same. `/trace now` names no command on either (`commands.zig:486` has no `has_args`), and neither wrote a report for it (three reports per run, one per bare `/trace`). |
| `trace-after-turn` | card, answer, `0s (↑4 ↓40)`, `* Trace copied to clipboard. …` | same | Rows 2-40 are byte for byte the same (rows 7-13 hold the turn and the second notice). |
| `trace-copy-fail` | `✗ Clipboard copy failed. Trace saved at <path>. Review and redact it before sharing.` | same sentence, wrapped differently | The glyph and colours are the same (`ESC[38;5;252m✗ ESC[38;5;250mClipboard copy failed. Trace saved at …`). The rows differ; see the first item below. No `.clipfile`: the shim refused and kept nothing, on both. |
| `trace-nowhere-unwritable` | `✗ Could not create trace.` | same | Row 3 is byte for byte the same (`ESC[38;5;252m✗ ESC[38;5;250mCould not create trace.ESC[39m`). No report and no copy on either. |

In every pair but `copy-fail`, the only row that differs is row 1, each
product's own banner (`𝒇x v0.0.13` vs `x𝒇x v0.1.0-cafab50 [debug]`).

## The reports

`*-report-1.md` is the first `/trace` (no turn yet), `-2` the one after the
turn, and `-3` the one the clipboard refused. All six were written with mode
600 (the capture log's `written mode 600`); the copies here are the receipt's
own files. They are 99/133/141 lines on fx and 47/73/76 on xfx.

What xfx's report holds, section by section:

- `## Summary`: when; `version: 0.1.0 (<revision>)`; platform; build channel;
  provider (and its URL, credentials redacted, for llmux); model; effort;
  permission mode; workspace.
- `## Current State`: session id, session directory and title, turns and
  token totals (from the session log), agent step limit, the credential **by
  its variable only** (`auth: source=AI_GATEWAY_API_KEY refreshable=false`),
  pid, and on the TUI the work in hand, the terminal size and the `/stats`
  counters. Before the first prompt it says
  `session: none (xfx opens one at the first prompt)`.
- `## Configuration`: the two settings files and whether each was read, the
  layer each setting came from, and how many diagnostics. No values.
- `## Problems`: missing credential, rejected provider, each configuration
  diagnostic, a session log that could not be read or recorded, interrupted
  turns, a turn the log lost the end of, failed tool calls, refused provider
  requests and error or warning notices. On the TUI, a report taken mid-turn
  says so. (Here: the `/trace now` error notice.)
- `## Network Calls` (TUI): the full transcript's `* network:` records.
- `## Tool Calls`, `## Permissions`: from the session log.
- `## Session Log`: the log turn by turn: each turn's times, outcome and
  output tokens, the prompt, what the model said, its calls, their results and
  any approval feedback.
- `## Transcript Timeline` (TUI): what the screen was given, under upstream's
  kind names (`user`, `assistant`, `semantic notice`, `raw`), colour stripped
  and the screen's gutter taken off.

Every piece of text that came from a person, a model, a tool or a settings
file is masked by upstream's rules (`maskSecrets`, `text_utils.zig:457-940`)
and by the credential shapes `scripts/check-no-secrets.sh` names. A `KEY=${VAR}`
assignment is left as written, as upstream leaves it.

fx's report has sections xfx has no facts for: `Last Shutdown`,
`Context Compaction`, `Session Title` generation, `Model Catalog`,
`Runtime Context`, `Renderer State`, `Recent Renderer Events`, a `ps` snapshot,
open fds, the worker state and the `FX_TRACE_LOG` tail. They are absent rather
than written empty.

## What differs, and why

1. **The `copy-fail` wrap.** fx breaks the path at a `/`
   (`…/fxtmp/` then `fx-trace-…md. Review and redact it before sharing.`, two
   rows) because its notice scanner treats `/` and `\` as break points and
   drops the space at a break (`scanNoticeLine`,
   `src/ui/render_engine/transcript_blocks.zig:1651-1702`). xfx breaks only at
   spaces and keeps the trailing space (`Trace saved at ` then the whole path,
   then `sharing.`, three rows). This is the shared notice wrap
   (`src/tui/shell.rs` `queue_notice` and the transcript's word wrap), not
   `/trace`'s, and any notice with a long path shows it. It was not changed
   here.
2. **No `Preparing trace...`.** fx writes that notice first and replaces it in
   place with the outcome (`app_commands.zig:624-660`). xfx's document is
   append-only, so the outcome is the only notice. The final screens are the
   same. fx's own report ends its timeline on `* Preparing trace...`. xfx's ends
   on the entry before `/trace`.
3. **Where the work happens.** fx builds, writes, syncs and copies on its UI
   thread. xfx takes the screen's facts at the keystroke and does the rest on a
   thread of its own (`src/tui/trace.rs`), which reads the session log back
   from disk. Every event is published as it is written, so the log is current
   to its last event.
4. **Timeline entries.** fx lists its banner as a `raw` entry. xfx's full
   transcript records the welcome line without a time, so it is left out.
5. **The file name** is `xfx-trace-…` rather than `fx-trace-…`. The shape,
   directory and mode are fx's.
