# T6 receipt: the turn summary's out count over a multi-step turn

Three scenarios, each at 40x120 on both binaries with the repo's own driver:

```
TMPDIR=/private/tmp/claude-501 FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
TMPDIR=/private/tmp/claude-501                                    scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `allowlist-turn` (two requests in its second turn), `toolgroup` (eight
requests in its first turn, the live row mid-turn too) and `undo-resume` (two
multi-request turns, **replayed** from the log after `--resume`). The xfx
binary is `target/debug/xfx` built from `489aa50`, which is what its banner
says. The driver, its shims and its fx sound guard are unchanged. The request
bodies `toolgroup`'s `gwlog` kept are not copied (they carry each product's
system prompt and the scratch workspace path); its gateway logs are.

## The rule, and where it was broken

fx's out is a sum over every model request of the turn -- each at what the
provider reported, or at the estimate of what that request streamed (answer,
reasoning and tool-call arguments, estimated apart) when it reported nothing,
failed, or was stopped -- plus the open request's estimate while it streams;
in is the prompt's estimate, fixed when the turn starts
(`vercel-labs/fx@c1db9191 src/core/agent/runtime/telemetry.zig:83-180`;
`orchestrator.zig:5482-5485`, `:7493`, `:7577`, `:8176`). A resumed session
shows the summary the turn saved (`session_event.zig:107`,
`app_session_runtime.zig:4776-4778`).

xfx showed the **answering** request's count alone: the worker sent one
`UiEvent::Usage` per turn from `TurnOutcome::usage`, which is the last
completion's (`agent/machine.rs` `drive`), and the log's replay read the same
answering-step count. Every fixture request reports 40, so a turn of `n`
requests read `↓40` in xfx and `↓(40·n)` in fx.

## Per scenario

### `allowlist-turn` (3 snaps, all identical)

Row 1 (the welcome line) dropped and trailing blanks trimmed, all three snaps
are identical in text **and** SGR. `ran`, rows 7 and 18:

| | fx | xfx before (W8 receipt) | xfx now |
|---|---|---|---|
| one request | `0s (↑2 ↓40)` | `1s (↑2 ↓40)` | `0s (↑2 ↓40)` |
| two requests | `0s (↑4 ↓80)` | `1s (↑4 ↓40)` | `0s (↑4 ↓80)` |

`files.files` differ only in each product's own state files, as W8's did.

### `toolgroup` (13 snaps; every token count equal)

Every `(↑ ↓)` on every snap, both binaries:

| snap | fx | xfx |
|---|---|---|
| `cmd-running` (live row, three requests settled) | `Running (2s) (↑8 ↓120)` | `Running (1s) (↑8 ↓120)` |
| `end`, `again-*`, `long-*` (the first turn's summary) | `13s (↑8 ↓320)` | `13s (↑8 ↓320)` |
| `long-running` (live row of the third turn) | `Running (1s) (↑4 ↓40)` | `• Running (1s) (↑4 ↓40)` |

Before this change F1's receipt had `13s (↑8 ↓40)` and a live `↓0`. The
elapsed seconds and the marker's blink phase are the timing of the snap. The
snaps still differ elsewhere, all of it F1's documented differences and none of
it this unit's: the failed read's wording (`Path not found` against `read_file
refused the path`), `What can fx do differently?` against `xfx`, and the
Ctrl-C on a running `sleep 4` (fx `Cancelled sleep 4` at once; xfx still
`Running`, then `Ran sleep 4` and a turn-level `■ Cancelled`), which also moves
the rows above it by one or two.

### `undo-resume` (the replay)

Two turns that each wrote a file, the session left, and resumed with
`--resume-last` / `--resume last`:

| | fx (W6 had `1s` on the second) | xfx before (W6 receipt) | xfx now |
|---|---|---|---|
| first turn, replayed | `0s (↑5 ↓80)` | `0s (↑5 ↓40)` | `0s (↑5 ↓80)` |
| second turn, replayed | `0s (↑5 ↓120)` | `0s (↑5 ↓40)` | `0s (↑5 ↓120)` |

The replayed counts come from the log: the usage frame now keeps the count the
live summary showed (`turn_output_tokens`), because a replay cannot count a
lost attempt or a request's reasoning again. The one other difference is the
replayed `Wrote a.txt +1` against `Wrote a.txt`: xfx's log does not keep a
change's line counts, which `/resume`'s parity row already says.
