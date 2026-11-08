# MCP-C receipt -- MCP config v2 (E1, A7, B5)

Unit MCP-C of the 0.0.16 delta (`.prd/tui-phase5/research/mcp.md` §5 unit 2 on
the research branch). Security-sensitive (S): file writes into the user's
repository. Base `feat/p5-fxcap-h1` `d5425d93`. Oracle fx v0.0.16 (`481104fd`,
executable sha256 `8e6cda2efddb…`, checked by capture.sh before and after every
fx run). xfx: `target/debug/xfx` from this branch, and the base built from
`git archive d5425d93` in a scratch directory (`x𝒇x v0.1.0 [debug]`).

## What changed

- `src/mcp/config.rs`
  - **Reader**: a profile entry xfx can't read is skipped with a note in fx's
    words instead of the whole profile refused (`Profile::notes`), and so is a
    name under `mcpServers` beside an `mcp`. The workspace's issues are fx
    v0.0.16's sentences too. `streamable-http` is `http`; `"url"` with no
    `"type"` is its own reason. Both files go through one strict JSON reading
    (a key twice is no JSON, as `std.json` has it).
  - **Writer** (`config_file.zig@481104fd`), an API for the `/mcp` verbs of
    MCP-V: `File`, `Entry`, `Change`, `Updated`, `EditError` (with `why()`, fx's
    words), `add_entry`, `remove_entry`, `defines`, `read_config`,
    `project_path`, `update`. In-place edit, numbers as written, `std.json`'s
    string spelling, two-space JSON; an old profile keeps its `mcp` key.
  - The typed `/mcp add` / `/mcp remove` (`add_profile_server`,
    `remove_profile_server`) are unchanged, and read the profile strictly: an
    entry the reader would skip stops them instead of being dropped.
- `src/mcp/runtime.rs` (MCP-S's file; the one shared call site,
  `read_configuration`, +10/-8): the profile's notes go ahead of the
  workspace's issues.
- `src/mcp/tests.rs`, `src/tui/mcp_menu.rs`: four expected strings now fx's
  sentences.
- `docs/parity.md`: rows "MCP profile `~/.xfx/mcp.json`" and "MCP project
  servers and trust".
- `scripts/fxcap/scenarios/p5-mcp-config.cap` (+ `-profile.json`, `-ws/`): the
  writer's oracle scenario.

## E1 + A7: the bytes the writer leaves

`oracle/` holds what fx v0.0.16 left on disk, cut from its captures: the
`home` verb's block for `~/.fx/mcp.json` (header size checked against the
bytes once `<bun>`/`<scenarios>` are put back: 568, 299, 378, 1137, 413), and
the `workls` text of `.mcp.json` (under its 2000-byte cut), redacted with
`scripts/fxcap/redact.py`. The unit tests compare with `include_str!`, `==`
on every byte.

| oracle | fx capture | xfx unit test (`src/mcp/config.rs`) | |
|---|---|---|---|
| `verbs-after-add.mcp.json`, `verbs-end.mcp.json` | `cap/fx/fx-p5-mcp-verbs-40x120-{after-add,end}.files` | `the_verbs_scenario_leaves_fx_v0_0_16s_bytes` (seed `p5-mcp-profile.json`; add `two` stdio, `web2` http; remove `two`, `three` (absent), `web2`) | identical |
| `config-{before,after-add,end}.mcp.json` | `cap/fx/fx-p5-mcp-config-40x120-{before,after-add,end}.files` | `an_old_profile_keeps_its_mcp_key_and_every_other_byte_fx_keeps` (old `{"mcp":…}` profile with `1.50`, `-0.5E+10`, `é`, `\/`, `\u001f`, a surrogate pair, `{}`, `[]`; add `two`, `three` with args and env, `web` with a header; remove `old`, `three`, `web`) | identical |
| `config-work-{before,after-add,end}.mcp.json` | `cap/fx/fx-p5-mcp-config-40x120-work-*.txt` | `a_project_file_is_edited_as_fx_edits_it` (4-space `.mcp.json` with an `other` key; add `p2`, remove it) | identical |

fx's own unit tests are ported beside them (`config_file.zig:234-391@481104fd`).

**RED** (base xfx on the same scenario, `cap/xfx-base/xfx-p5-mcp-config-40x120-after-add.files`):
one line of `{"mcp":{…}}`; `theme`, `n`, `e`, `s`, `empty`, `list`, `nested`
gone; `--env`, `--header` and `--project` saved as server names with the rest
of each line as their command; `.mcp.json` untouched, the project add written
into the profile (`work-after-add.txt`).

**The migration unknown** (research `## Unknowns`): fx v0.0.16 reads a
Claude-shape entry it wrote under an old `mcp` key --
`cap/fx/fx-p5-mcp-config-40x120-list.txt`: `two  ~/.fx/mcp.json  stdio  ready, 1 tool`.
xfx's reader takes it too (same test, `parse_profile` of the result).

## B5: an entry xfx can't read is a note, not a refused launch

`p5-mcp-notes` (`{"fixture":…,"broken":{"command":123}}`), cell by cell with
`.prd/tui-phase4/receipts/m1/cells.py` (row 1 not compared, `/.fx/` read as
`/.xfx/`):

| snap | fx vs xfx base | fx vs xfx head | rows left, and the unit that closes them |
|---|---|---|---|
| `startup` | 5 rows (`! mcp: MCP configuration could not be reloaded…`, no server) | **1 row**: row 5's right side, `MCP config has 1 problem · /mcp list` | MCP-S (C2, status line) |
| `menu` | 10 rows (`MCP 0`, `No MCP servers configured.`) | 4 rows: `fixture  Ready` and the note row are there, in M1's menu | MCP-M (B1, B5 row `1 config note: /mcp list shows it`) |
| `list` | 8 rows | 8 rows: xfx's `/mcp list` still opens M1's menu | MCP-V (A4 table, `! mcp: note: …`) |

The note's sentence is pinned to fx's capture:
`a_profile_entry_xfx_cant_read_is_skipped_with_fxs_note` reads the
`! mcp: note: ` row of `cap/fx/fx-p5-mcp-notes-40x120-list.txt` and asserts
xfx's note is it, `/.fx/` read as `/.xfx/`:
`~/.xfx/mcp.json server 'broken' was skipped: it needs a "command", with any "args" as strings.`

## Security (S)

| property | test |
|---|---|
| every `--project` write goes to a throwaway repository under the run's TMPDIR (asserted outside `CARGO_MANIFEST_DIR`), never the checkout | `scratch_repo()` in each `update` test |
| atomic: a `create_new` stage beside the file, synced, renamed over it; the stage removed on any failure; nothing written on any error | `an_edit_that_fails_writes_nothing` (Exists, a key twice, bad JSON, 10,000-deep nesting, servers not an object, not UTF-8, read-only, over 1 MiB, a directory, no workspace), directory listings compared |
| the file keeps its mode (0664 kept; a new one at 0666 less the umask); the profile 0600 in a 0700 directory | `a_project_edit_replaces_the_file_whole_and_keeps_its_mode`, `the_profiles_first_write_creates_its_directory_readable_only_by_its_owner` |
| a symlink (inside, outside, dangling) or a file with another hard link is refused before it is read, for both files; the file it points to unchanged (one target isn't JSON, so a read through the link would answer `Unreadable`) | `a_linked_config_file_is_never_edited_whether_a_symlink_or_a_hard_link` |
| names and link targets reach a sentence only through `crate::mcp::terminal_safe` (a note's name cut at 160 bytes, fx's) | `a_name_in_a_note_is_spelled_for_a_terminal_and_cut_at_160_bytes`; the link test's control-character target |
| a hostile file cannot run the reader out of stack: 127 levels, `serde_json`'s bound, arrays and objects each, for both readers | `the_reader_and_the_writer_take_the_same_depth` (127 taken, 128 and 200,000 refused) |
| the writer's JSON reader takes what `serde_json` takes, and writes back the same value | `the_writer_reads_json_as_serde_json_does_and_writes_back_the_same_values` (40,000 generated documents) |

## MCP-Cb: the review round

Both engines' findings (`.../p5/MCP-C/reviews/gpt6.md`, opus's in the brief):

| finding | fix | RED (old code) | GREEN |
|---|---|---|---|
| `read_bounded` looked at the path, then read it again unbounded and blocking | one descriptor, opened `O_NONBLOCK`, `fstat`ed as a regular file, read through `read_capped` (at most 1 MiB + 1) | with the look-then-read seam (`read_bounded_then`) in the old shape: grown file read 2,097,154 bytes; swapped file read the 2 MiB replacement; FIFO swap blocked, killed at 15 s (exit 137, `timeout -s KILL` in its own process group) | `a_config_that_grows_…`, `a_config_swapped_…`, `a_config_swapped_for_a_fifo_…`, `a_reader_reads_no_more_than_the_cap_…` (`io::repeat`), `a_fifo_where_a_config_should_be_…` |
| `update` wrote output past the cap | `render` stops at 1 MiB and returns `EditError::TooLarge`; the original stays | with the check removed: `left: Ok(Added)`, `right: Err(TooLarge)` | `an_edit_that_would_leave_a_file_over_the_cap_…` (add and remove; a 127-deep 400,000-zero file writes under 2 MiB before it stops) |
| object depth cap untested | objects beside arrays in the depth test | object cap removed: fails at 128 deep | `the_reader_and_the_writer_take_the_same_depth` |
| stage made at 0666 then narrowed; mode from a second `lstat` | stage born at the target's mode, its exact bits after; mode from the descriptor `read_plain` read | | `a_project_edit_replaces_…` (+ no execute bit) |
| stage guard armed before `create_new` | guard made after; `replace_through` takes the stage name | | `a_stage_name_already_there_is_never_removed` |
| no failure after the stage exists was forced | `StageStep` seam, test builds only | | `a_write_that_fails_after_its_stage_exists_…` (write, sync, rename) |
| profile write `Failed` after its rename landed; unreadable link target given the hard-link sentence | the profile goes through `replace_staged` too (dir sync best effort); `EditError::Symlink(None)` has its own sentence, `HardLink` its own | | the link test |

## MCP-Cc: the last round

| finding | fix | RED | GREEN |
|---|---|---|---|
| MCP-Cb opened a `.mcp.json -> /dev/<device>` before refusing it (b2b8c037 refused it unopened) | `fs::metadata` through the links before the open; the descriptor's `fstat` still decides | on ea9eac60's logic with the seam (`ReadStep::Opening`): `/dev/null was opened` | `a_config_linked_to_a_device_is_refused_without_being_opened` (`/dev/null`, `/dev/zero`, `/dev/tty`, a FIFO, a directory, links to them; a plain file and a link to one still read) |
| the profile's 0600 after an edit was unpinned | test | `Some(plain.mode)` for the profile: `from 644` fails | `a_profile_is_owner_only_after_an_edit_whatever_mode_it_had` (0644 and 0666 before) |
| an `O_NONBLOCK` revert would hang the FIFO tests | both run on a thread with `recv_timeout(10 s)`, a blocked read let go by a writer; the swap now also lands between the first look and the open, where only `O_NONBLOCK` keeps the open from waiting | `O_NONBLOCK` removed: `the read blocked on … for 10 s`, exit 101 in 13 s | `a_config_swapped_for_a_fifo_…`, `a_fifo_where_a_config_should_be_…` |

## MCP-Cd: the test helper

`within_seconds` (test-only) now resumes a read's own panic on `Disconnected`, releases a blocked read with an `O_NONBLOCK` writer (`ENXIO` with no reader, never a wait), keeps that writer open until the read answers, and joins the thread, or names it in the failure when it still runs. RED on the old helper, each run in its own process group under `timeout -s KILL 20`: `a_read_that_panics_fails_its_test_at_once_with_its_own_panic` and `a_slow_read_with_no_reader_on_the_fifo_does_not_block_the_release` killed at 20 s (exit 137); GREEN in 1 s and 2 s, with `a_read_blocked_on_the_fifo_is_let_go_joined_and_reported`. The release without `O_NONBLOCK` hangs again (killed at 20 s).

## For other units

- MCP-V: print `Couldn't add X to FILE: <EditError::why()>`; refuse `--header Authorization=…` with `--project` before `update` (xfx's reader skips such a `.mcp.json` entry); delete `render_profile` / `add_profile_server` / `remove_profile_server` once the verbs call `update`.
- MCP-M: M1's menu calls a profile note a "project MCP configuration error".
- MCP-S: the `startup` pair's last row, `MCP config has N problem(s) · /mcp list`.

## Reproduce

```
FX_BIN=<fx v0.0.16> TMPDIR=<tmp> scripts/fxcap/capture.sh fx  p5-mcp-config 40x120 <out>
FX_BIN=<fx v0.0.16> TMPDIR=<tmp> scripts/fxcap/capture.sh fx  p5-mcp-verbs  40x120 <out>
FX_BIN=<fx v0.0.16> TMPDIR=<tmp> scripts/fxcap/capture.sh fx  p5-mcp-notes  40x120 <out>
                    TMPDIR=<tmp> scripts/fxcap/capture.sh xfx p5-mcp-notes  40x120 <out>
cargo test --locked --lib mcp::config
```

## Gates (this branch after MCP-Cc, rustc 1.99.1)

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4576 passed, 0 failed, 7 ignored (14 suites) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4597 passed, 0 failed, 7 ignored (14 suites) |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1091 checks, 0 failures |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
