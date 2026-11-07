# P5 T-U6 receipt: T7, T8, T10

Epic 4 (Transcript), unit T-U6: fx v0.0.16 (`vercel-labs/fx@481104fd`) T7, T8
and T10 of `research/tui-phase5:.prd/tui-phase5/research/transcript.md` (§2;
§3 row U6). Meant as receipts only. T7 and T8 held on the base. T10's
security claim did not, so this unit has one code change. **S**: file content
reaching the terminal.

- **Base:** `feat/p5-transcript-receipts` at `039fb5c8`, PR #70's head
  (T-U1+U2 and T-U5). `origin/main` (`10434d28`) is an ancestor.
- **Builds:**
  - every `xfx/` capture is from a debug build of `4aca2576`, the code commit
    under this receipt;
  - every `red/` capture is from a build of the base `039fb5c8`.
  - `publish.py` checked each xfx snap whose row 1 is the banner as it copied
    it: `v0.1.0-4aca257` and `v0.1.0-039fb5c`. Every scenario has at least one
    such snap.
  - The submit wires carry no banner on screen, so `check.py`'s `build`
    claim finds it in the wire's own bytes, once.
- **fx:** the v0.0.16 reference binary (sha256 `8e6cda2e…`). Every run's guard
  line says it was unchanged.

## What changed (`4aca2576`)

- **`src/tui/full_transcript.rs` `diff_rows`** (Ctrl-O): each line of a change
  now goes through `approval_view::tamed` before it is laid out
  (`:1930-1936`).
  - The permission boundary (`bounded_diff_side`,
    `src/permission/authority.rs:737`) names a change's **controls**.
  - It does not name a bidirectional override, a zero-width space or a
    byte-order mark: none of them is `char::is_control`.
  - `bridge::inert` (`src/tui/bridge.rs:1477`) blanks controls only, and
    `frame::tamed` (`src/tui/frame.rs:2853`) strips only escape sequences. So
    those characters reached the terminal as themselves, on this screen
    only.
  - The review already names them (`Review::rows` → `safe_rows` → `tamed`,
    `src/tui/approval_view.rs:543`). Ctrl-O now spells a line as the review
    does.
  - fx v0.0.16 escapes them too (`appendTerminalSafeLine`,
    `src/core/shared/text_utils.zig:419-442@481104fd`, called at
    `src/core/output/diff.zig:1273@481104fd`; `isNonPrintingCodepoint`,
    `:703-708`).
- **The test:**
  `a_changes_rows_name_what_would_reorder_or_hide_its_text_as_the_review_does`.
  It checks that no raw U+202E, U+200B, U+FEFF, ESC or C1 reaches a row; the
  exact rows; and that the review shows the same words for each line.
- **Scenarios** (`scripts/fxcap/scenarios/`):
  - `p5-transcript-diffctl`: xfx gets its own turns. xfx's `edit_file`
    refuses a file it has not read in full, so it reads `ctl.txt` first. It
    also gets `XFX_PERMISSION_MODE=yolo`. fx's lines are unchanged.
    T-U1U2's `regressions.txt` ran this scenario with fx's turns, so its xfx
    edit was refused then.
  - New:
    - `p5-transcript-diffodd`: a file with a tab, U+202E, U+200B, U+FEFF, a
      CRLF line, a DEL and a literal `\x1b`;
    - `p5-transcript-diffbad`: a file that is not UTF-8;
    - `p5-transcript-diffask`: `ctl.txt` reviewed in `ask` mode, then
      applied;
    - `p5-transcript-submit-multi`: a three-line prompt on a full screen.
  - Their files are in `p5-transcript-diffws/`.
- **`docs/parity.md`:**
  - the full-screen TUI row: the submit frame and the tall prompt;
  - the Ctrl-O row: the spelling, and the open finding below;
  - the `edit_file` row: a file that is not UTF-8.
- **transcript-wrap's `check.py` hardening** (T-U1+U2's review). See that
  README's "Round 4" section:
  - `record()` pins the footer to row 38;
  - `check_insert` pins the inserted record's row and height;
  - `mutants-c.py` asserts its own outcomes;
  - `README.md:227` is scoped to its exceptions.

## RED

- **Test** (`red-test.txt`): on `039fb5c8`'s code with only the test added,
  `'\u{202e}' reached a row`. Row 2 was `"  │ 1 + bidi \u{202e} x"`.
- **Captures** (`red.txt`, `check.py … red`): 31 claims, 30 hold, **1 fails**:
  `spelled p5-transcript-diffodd full`.
  - Rows 27 and 28 hand the terminal U+202E, then U+200B and U+FEFF, as
    themselves.
  - Every other claim holds on the base too, T7 and T8 included: nothing else
    changed.

## GREEN (`green.txt`: 31 claims, 31 hold)

`check.py` reads every capture cell by cell, styles included. Where it
compares two sides, it applies the ledger's normalizations first (`banner`,
`turn-wall-time`, `activity-pulse`, `activity-clock`). It sets nothing else
aside unless the claim names it.

### T7: the card is in the frame that clears the composer (70611a91)

- **How the frames were read.** `frames.py` replays each side's wire into a
  private tmux pane up to the end of frame 1, and of frame 2. The frames are
  the synchronized updates after the before-wire's end.
  - The `replay` claim: the whole after-wire replayed is the `second` snap
    taken just before that wire was copied, every row, on both sides. The
    replay is the capture's terminal.
- **`p5-transcript-submit`:**
  - frame 1, on both sides: composer row 38 is `┃` alone, and `┃ second
    prompt` is at row 34, one blank row under the previous turn's summary.
  - fx's frame 1 against xfx's: rows 1-35 and 37-40 are the same.
  - Row 36, the activity row, is set aside by name: fx `• Thinking` (its
    clock comes a frame later) against xfx `Thinking (<s>) (↑4 ↓0)`.
  - The 0.15 s snap is compared the same way. Whether the answer's reported
    tokens were already there depends on when the snap fell.
  - `filled` and `second` are identical.
  - The xfx code path: `submit` → `send` → `echo` writes the card in the
    same `route_bytes` that takes the draft (`src/tui/shell.rs:11617`,
    `:11812`).
- **`p5-transcript-submit-multi`:**
  - fx's frame 1 previews only `┃ multi line three`; its frame 2 writes all
    three rows (32-34).
  - xfx's frame 1 writes all three.
  - fx's frame 2 and xfx's frame 1 are the same but for the activity row.
  - `filled`, `second` and `second.full` are identical.
  - So the difference lasts one frame, a few milliseconds. It is recorded in
    `docs/parity.md` (the full-screen TUI row).

### T8: a tall first prompt reaches scrollback whole (433464dd)

- `p5-transcript-tallfirst` `tall.full`, on both sides: all 60 card rows
  (`TALL_PROMPT_HEAD`, bodies 01-58, `TALL_PROMPT_TAIL`) appear once each, in
  order, one after another. They are rows 3-62, under the banner and one
  blank row. `tall` and `tall.full` are identical on the two sides.
- With an earlier turn above it (`p5-transcript-submit` `tall.full`): the
  same 60 rows at 73-132, under the previous turn's summary. Identical on
  the two sides.

### T10: a file's bytes in diff rows; Ctrl-O does not exit (9db199e4)

- **`spelled`:** every diff row is the fixture's line, as that product spells
  a line:
  - `fx_spelled` ports `appendTerminalSafeLine`;
  - `xfx_spelled` is `bounded_diff_side`, then `tamed`.

  The rows are exactly the edit's window, and no cell holds a control or
  anything fx's `isNonPrintingCodepoint` names. That holds on:
  - Ctrl-O over `ctl.txt` (`diffctl`, and `diffask` after it was applied);
  - the review in the `ask` question (`diffask` `ask`);
  - Ctrl-O over `odd.txt` (`diffodd`).

  Ctrl-O stays open and closes again on both sides, as the `closed` snaps
  show.
- **Rows with the same text** are the same cell by cell. Rows spelled apart
  have fx's styles. They are:

| line | fx v0.0.16 | xfx |
|---|---|---|
| `before ESC[2Jcleared` | `before\x1b[2Jcleared` | `before\u{001B}[2Jcleared` |
| `second U+009B line` | `second \u{009b}line` | `second \u{009B}line` |
| `tab<TAB>here` | `tab     here` (the tab kept) | `tab\there` |
| `bidi U+202E reversed` | `bidi \u{202e} reversed` | `bidi \u{202E} reversed` |
| `zero U+200B width U+FEFF bom` | `zero\u{200b}width \u{feff} bom` | `zero\u{200B}width \u{FEFF} bom` |
| `crlf line<CR>` | `crlf line` (the CR dropped) | `crlf line\r` |
| `del DEL and literal \x1b text` | `del \x7f and literal \x1b text` | `del \u{007F} and literal \\x1b text` |

- **`refused` (`diffbad`, not UTF-8):**
  - fx edits the file and shows `bad \xff byte` and `lone \xc3 lead`.
  - xfx draws no diff. Its call fails: `└ Failed bad.txt: edit_file will not
    replace …`. Its read was `binary or non-utf8 file omitted (34 bytes)`
    (`src/tools/read.rs:1162`), so the read proof refuses the edit before
    `edit_file cannot edit … it is not UTF-8 text` would
    (`src/tools/mutate.rs:437-442`).
  - A `write_file` over such a file reviews its before as `<N bytes of
    non-UTF-8 data>` (`mutate.rs:659-661`).
  - So no byte of such a file reaches any diff.
- **`nodiff` and `inline`:**
  - Neither product's inline transcript draws a diff: `└ Edited <file> +1 /
    -1` and no diff row.
  - `diffodd` `edited` and `closed`, and `diffask` `applied` and `closed`,
    are identical on the two sides.

### T10 security: every diff surface, from the code

`Review` has three consumers, and they are all of xfx's diff rows:
- the band's question, `src/tui/approval.rs:329`, `:490`;
- the review plane, `src/tui/approval_screen.rs:142`, `:337`;
- Ctrl-O, `src/tui/full_transcript.rs:1908`.

1. **Band and plane:** `Review::rows` → `safe_rows` → `tamed` (`approval_view.rs:543`). This was already safe.
2. **Ctrl-O `diff_rows`:** `tamed` since `4aca2576`. It was not safe before;
   see RED.
3. **Inline:** none. The transcript row for a made change is its summary
   only.

On every path the sides come from `ApprovalDiff`, whose two `String`s are
valid UTF-8 by type and already escaped (`bounded_diff_side`). A file that is
not UTF-8 never becomes one (above).

## Divergences and the open decision

- **The spelling (open, user decision).** xfx spells a change's bytes the
  injective way its review already does (the full-screen TUI row of
  `docs/parity.md`). fx v0.0.16 uses `\x1b`, lower-case `\u{009b}`, a kept
  tab, a dropped trailing CR and a single backslash. Three options:
  - **A.** Keep xfx's spelling on both screens (this branch). It is
    injective, so a file holding a literal `\x1b` is not shown as one holding
    an ESC, and one change reads the same in the question and in Ctrl-O.
  - **B.** fx's forms in Ctrl-O only. Byte parity there, but Ctrl-O and the
    review would spell one change two ways.
  - **C.** fx's forms on both. Full parity, but the review loses
    injectivity, a documented security property. This would be a change to
    `bounded_diff_side` and every surface that reads it.
- **The submit transient** (decided, recorded). xfx writes a multi-line
  card whole in the first frame. fx previews its last line for one frame.
- **A file that is not UTF-8** (recorded, `edit_file` row). xfx does not edit
  one; fx v0.0.16 does.

## Findings no claim covers (printed by `check.py`)

- **Ctrl-O read, argument and command rows** are a different path from the
  diff:
  - `Detail::made_inert` → `bridge::inert`, then `railed`/`literal`.
  - They blank a control to a space: `before [2Jcleared`, where fx v0.0.16
    shows `before\x1b[2Jcleared`.
  - They let U+202E, U+200B and U+FEFF reach the terminal as themselves.
    `p5-transcript-diffodd` `full` rows 16-17 show it.
- These rows are outside T10's diff scope. They are the Ctrl-O tool-row
  path, which T-U1 just rewrote. They also raise the same spelling question
  as the decision above. So they are left as a separate unit, recorded open
  in the Ctrl-O row of `docs/parity.md`.

## Files

- `fx/`, `xfx/`, `red/`: the captures, redacted with `scripts/fxcap/redact.py`;
  the wires are copied as bytes after a local-root check. Each
  `-frame1`/`-frame2`/`-wireend.ansi` file is made by `frames.py` from that
  side's wires.
- `publish.py`, `frames.py`, `check.py`, `mutants.py`: the scripts.
- `green.txt`, `red.txt`: `check.py`'s output on `xfx/` and on `red/`.
- `mutants.txt` is `mutants.py`'s output. It has 9 mutants: T7 ×3, T8 ×2,
  T10 ×4. Each fails with the FAIL line it names, the unchanged captures
  pass, and the run exits 0.
- `red-test.txt`: the RED unit-test run.

Commands, from the worktree:

```
python3 -I -B .prd/tui-phase5/receipts/transcript-receipts/publish.py . <scratch> .prd/tui-phase5/receipts/transcript-receipts 4aca257 039fb5c
python3 -I -B .prd/tui-phase5/receipts/transcript-receipts/frames.py .prd/tui-phase5/receipts/transcript-receipts /private/tmp/claude-501/tr/fr
python3 -I -B .prd/tui-phase5/receipts/transcript-receipts/check.py . .prd/tui-phase5/receipts/transcript-receipts xfx   # green.txt
python3 -I -B .prd/tui-phase5/receipts/transcript-receipts/check.py . .prd/tui-phase5/receipts/transcript-receipts red   # red.txt
python3 -I -B .prd/tui-phase5/receipts/transcript-receipts/mutants.py . .prd/tui-phase5/receipts/transcript-receipts /private/tmp/claude-501/tr/mu
```

## Gates

On `4aca2576`'s code (this receipt's commit adds only the receipt), with
`TMPDIR=/private/tmp/claude-501/tr`. The suites ran one at a time, after the
idle wait.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4926 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4955 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh` | ok, ok |
| `scripts/check-no-secrets.sh`, run again once this receipt was staged whole | ok |
| `check.py … xfx` (`green.txt`) | 31 claims, 31 hold |
| `check.py … red` (`red.txt`) | 31 claims, 30 hold, 1 fail (`spelled p5-transcript-diffodd full`) |
| `mutants.py` (`mutants.txt`) | 9 mutants, each failed with its line; exit 0 |
| transcript-wrap `check.py` on its captures | 34 claims, 34 hold (`green.txt` unchanged); on `red/` 34 claims, 12 fail (`red.txt` unchanged) |
| transcript-wrap `mutants-c.py` (`mutants-c.txt`; `mutants-c-meta.txt`) | 9 mutants, every assertion held, exit 0; with round 3's `check.py` in its place, 3 broke, exit 1 |
