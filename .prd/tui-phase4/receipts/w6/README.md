# W6 `/undo` — fx `c1db919` vs xfx, 40x120

Three scenarios, driven identically on both binaries by `scripts/fxcap/capture.sh` (isolated tmux
socket, empty `HOME`, fake key, loopback fake gateway, desktop shims and fx's sound guard kept). Both
run in the harness's `auto` mode. The xfx captures were taken at `ba90ad2` (this branch with
`plan/tui-phase4` at `2b5967f` merged in); fx is the `c1db919` build.

```
export TMPDIR=<tmp>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  undo        40x120 <out>
scripts/fxcap/capture.sh                            xfx undo        40x120 <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  undo-empty  40x120 <out>
scripts/fxcap/capture.sh                            xfx undo-empty  40x120 <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  undo-resume 40x120 <out>
scripts/fxcap/capture.sh                            xfx undo-resume 40x120 <out>
```

The run directory (`/private/tmp/claude-<uid>/fxcap.XXXXXX`) is written `<scratch>` in every file here;
nothing else was changed. `*-work-<label>.txt` is the workspace's files at that point (`workls`).

The turns (`undo-turns.json`, one file for both): `write one please` → `write_file a.txt` = `one\n`,
then `wrote a.txt (one)`; `write two please` → `read_file a.txt`, `write_file a.txt` = `two\n`, then
`wrote a.txt (two)`. The read is xfx's: its `write_file` refuses to replace a file the session has not
read (SSOT §Tensions, "Tool backend is not UI"); fx takes the read as well, so the two run the same
calls and draw the same tool group (`● 2 tool calls · 1 read · 1 write`).

## `undo` — two changes taken back, then nothing

| snap | result |
|---|---|
| `before-undo` | the same screen but for #1 and #2: both turns' groups (`└ Wrote a.txt +1`, `├ Read a.txt` / `└ Wrote a.txt +1 / -1`), answers, footer. Files: `a.txt` = `two` on both. |
| `undo1` | **the notice row is byte-identical**: `ESC[38;5;250m* undo: Restored <scratch>/work/a.txtESC[39m` (whole row 250, neutral; the path absolute). Files: `a.txt` = `one` on both (`work-after-undo1`). |
| `undo2` | identical row: `ESC[38;5;250m* undo: Deleted <scratch>/work/a.txt (was newly created)ESC[39m`, a blank row above it. Files: none on both (`work-after-undo2`). |
| `undo3` | identical row: `ESC[38;5;250m* undo: Nothing to undo.ESC[39m`. Files: none (`work-after-undo3`). The composer row and the hint row (`┃`, `auto · model-a`) are the same on both. |

## `undo-empty` — nothing tracked, and an argument

| snap | result |
|---|---|
| `bare` | identical after the banner: `ESC[38;5;250m* undo: Nothing to undo.ESC[39m`. |
| `extra` | `/undo now` is not `/undo` on either (`commands.zig:481` gives the row no `accepts_payload`): identical `ESC[38;5;252m✗ command:ESC[38;5;250m Unknown command. Try /help.ESC[39m` under the first notice. |

## `undo-resume` — the journal does not survive a resume

Both turns, `/quit`, the same pane relaunched on the session (`fx --resume-last`, `xfx --resume last`),
then `/undo`.

| snap | result |
|---|---|
| `resumed-undo` | **`ESC[38;5;250m* undo: Nothing to undo.ESC[39m` on both**, and `a.txt` = `two` on both (`work-after-resumed-undo`). fx's tracker is process-local and cleared by the transition `/new`, `/clear`, `/reset` and `/resume` share (`app_session_runtime.zig:1922` → `:1999`); its own resume test asserts the same answer with the files untouched (`tests/e2e/tui-resume.test.ts:6029-6037`). xfx keeps the journal in the conversation's tool session and writes nothing of it to the session log, so a resumed conversation starts with none. The replayed rows above it differ (#3). |

## What differs, and why

| # | fx | xfx | why |
|---|---|---|---|
| 1 | welcome `𝒇x v0.0.13` | `x𝒇x v0.1.0-ba90ad2 [debug]` | the product's own name and build. |
| 2 | turn summaries `0s (↑5 ↓80)`, `0s (↑5 ↓120)` | `0s (↑5 ↓40)` both | the summary's out count, the same difference every receipt since F5 shows (F5 README #4); not this unit's surface. |
| 3 | resumed rows `└ Wrote a.txt +1`, `└ Wrote a.txt +1 / -1` | `└ Wrote a.txt` | fx persists a `committed_file_presentation` per change and replays its counts; xfx's log keeps no diff (F5 README, "a change made before a resume has no diff"). Resume replay, not `/undo`. |

## What the captures cannot show, and how it was matched

Each of these is pinned by a test named in the report rather than by a screen.

- **No question in any mode.** fx's handler calls `undoLast` straight from the router
  (`app_commands.zig:366-392`, `:1242-1290`); xfx's `/undo` asks nothing and mints no authority, in
  `ask` too (`tools::mutate::tests::undo_asks_no_question_even_in_ask_mode`). The structural rules of
  a write hold: the target is located again from the conversation's scope, read without following a
  link, staged and renamed in its own directory with its mode kept, or unlinked there; a directory on
  the path that has gone is not made again (fx: `change_tracker.zig:399-423`).
- **Stricter than fx: the read-proof rule.** fx restores over whatever is at the path
  (`change_tracker.zig:53-81`; its test replaces a symbolic link with the preimage, `:501-537`). xfx
  refuses, with fx's own `Could not undo <path>`, when the file is no longer what the change left there
  or what the session last read -- the rule every xfx write obeys -- and leaves the file as it is; the
  change is spent either way, as fx spends it.
- **Mid-turn.** fx's `/undo` runs at once whatever its worker is doing. xfx's is a control message to
  the runtime thread, done at once between turns and, typed while a turn runs, the moment that turn has
  said its last word -- never between one call's commit and the next call's read. **What it takes back
  is pinned at the keystroke**: the band sends the journal's position then (`Mark::now`), and the
  runtime takes back the newest change at or before it, so a change the turn commits afterwards stays
  for the next `/undo`, as it would on fx; if that later change touched the same file, the answer is
  `Could not undo <path>` and the file is left as the later change made it (`tests/tui.rs`
  `an_undo_typed_while_a_turn_runs_takes_back_what_was_newest_then_once_the_turn_is_over`,
  `a_change_the_turn_makes_after_an_undo_is_typed_is_not_the_one_taken_back`,
  `an_undo_typed_before_the_turn_changed_the_same_file_again_takes_nothing_back`). Never refused,
  never queued. One typed after a `/new` the runtime has not reached yet is answered for the fresh
  conversation; its answer is never dropped by a reset's drain.
- **Provider switch.** xfx drops the conversation on a provider switch (its history cannot be replayed
  into another wire format), and the journal with it; fx's `/provider` keeps its session and tracker.
- **Byte bound.** fx keeps 100 entries whatever their size; xfx also drops the oldest past 64 MiB of
  preimage, always keeping the newest.
- **Hostile names.** fx spells a control byte in the path as text (`name\x1b]2;FX_PWN\x07\x0afile.txt`,
  `tests/e2e/tui-permissions.test.ts:1941-1982`); xfx spells it the same way, and a byte that is not
  UTF-8 as `\xNN` (`terminalSafeToken`, `text_utils.zig:560-584`). Not captured (the harness writes
  through tmux), pinned by `tools::undo::tests::a_hostile_name_is_spelled_out_as_upstream_spells_it`.

## Smoke rows

`scripts/smoke-tui.sh` types no `/undo` and reads no `undo:` row -- its undo rows are the composer's
`C-_` edit history, which this change does not touch -- and counts no slash command; no smoke row is
invalidated by this change.
