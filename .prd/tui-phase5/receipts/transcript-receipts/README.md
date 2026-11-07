# P5 T-U6 receipt: T7, T8, T10 (with T-U6b)

Epic 4 (Transcript), unit T-U6: fx v0.0.16 (`vercel-labs/fx@481104fd`) T7, T8
and T10 of `research/tui-phase5:.prd/tui-phase5/research/transcript.md` (§2;
§3 row U6). It was meant as receipts only:

- T7 and T8 held on the base.
- T10's security claim did not (T-U6). After that the dispatcher chose
  option B for its spelling (T-U6b): Ctrl-O's diff rows are fx's, byte for
  byte, and the review keeps xfx's spelling.

**S**: file content reaching the terminal.

- **Base:** `feat/p5-transcript-receipts` at `039fb5c8`, PR #70's head
  (T-U1+U2 and T-U5). `origin/main` (`10434d28`) is an ancestor.
- **Builds.** Three xfx sides, each from a debug build of the commit named:
  - `xfx/`: `f84734dc`, the code under this receipt (T-U6b's two commits,
    `798ac1f1` and `f84734dc`, on top of T-U6's `4aca2576`);
  - `red-u6b/`: `4aca2576`, T-U6, the head T-U6b changes;
  - `red/`: `039fb5c8`, the base.
  - `publish.py` checked each xfx snap whose row 1 is the banner as it copied
    it: `v0.1.0-f84734d`, `-4aca257` and `-039fb5c`. Every scenario has at
    least one such snap.
  - The submit wires carry no banner on screen, so `check.py`'s `build`
    claim finds it in the wire's own bytes, once.
- **fx:** the v0.0.16 reference binary (sha256 `8e6cda2e…`). Every run's guard
  line says it was unchanged.

## What changed

### Ctrl-O's diff rows (`src/tui/full_transcript.rs`)

`diff_rows`, Ctrl-O only, writes each line of a change as fx v0.0.16's
`appendTerminalSafeLine` does (`src/core/shared/text_utils.zig:419-442@481104fd`,
called at `src/core/output/diff.zig:1273@481104fd`). It takes three steps:

1. **`unspelled`** reads the line back from the permission boundary's
   spelling (`bounded_diff_side`, `src/permission/authority.rs:737`):
   - `\\` → `\`, `\t` → tab, `\r` → CR, and `\u{…}` → the code point it
     names;
   - a backslash that begins none of those stays as it is;
   - the boundary's spelling is injective, and this is its inverse on a line
     (tested for every scalar to U+00FF).
2. **`terminal_safe_line`** spells the line again in fx's forms:
   - a trailing CR is dropped (`:429`) and a tab is kept (`:432-436`);
   - a C0 control or DEL is `\xNN` (`:645-646`, `:675-686`);
   - fx's non-printing set is `\u{NNNN}`, lower case
     (`isNonPrintingCodepoint`, `:703-708`; `:688-701`). That set covers C1,
     U+200B-200F, U+2028-202E, U+2060-206F and U+FEFF;
   - everything else is itself (`:666`);
   - **one addition**: U+061C, which fx writes as itself, is `\u{061c}`
     (below).
3. **`drawn_tabs`** draws each tab as the terminal draws the one fx writes:
   blank, unstyled cells up to the next multiple of 8, then the row's style
   again. This screen places rows cell by cell, so it writes the cells a
   tab moves over.

What did not change:
- `bounded_diff_side`, `approval_view::tamed` and the approval band and
  plane;
- the review (approval) screen, which keeps xfx's spelling.

T-U6's first fix (`4aca2576`) had put Ctrl-O through `tamed`, the review's
spelling. That closed the security hole, and T-U6b replaces it.

### Tests

- `a_changes_rows_write_a_files_bytes_as_fx_does_and_hand_the_terminal_none`:
  the rows' exact bytes, the tab drawn, and no row holding a character a
  terminal acts on. That set is written out in the test, not read from the
  code (`f84734dc`). The test also checks that the review keeps xfx's
  spelling.
- `a_files_line_is_spelled_as_fxs_append_terminal_safe_line_spells_it`: a
  table with fx's line for each case:
  - ESC as a CSI, an OSC with its BEL, C1 U+009B and U+009D, DEL;
  - a tab, a trailing CR, two trailing CRs, a CR mid-line;
  - U+202E, U+2066/U+2069, U+200E/U+200F, U+200B, U+2028, U+FEFF;
  - non-ASCII printables, a literal backslash, and U+061C.
  - An invalid byte cannot occur: the line is a `str`. A file that is not
    UTF-8 is never edited (below).
- `the_boundarys_spelling_reads_back_to_the_files_text`: the round trip, and
  tokens the boundary never writes (`\q`, `\u{D800}`, `\u{zz}`, `\u{+41}`,
  a trailing `\`) kept as text.

### Elsewhere

- **The ledger** (`scripts/fxcap/parity-ledger.toml`, pin v0.0.16). The two
  approved divergences each carry an owner, a resolution and the expiry
  `date:2026-12-04`:
  - `review-diff-spelling` (unsafe-to-copy, owner T-U6b). The review keeps
    xfx's injective spelling, because fx's lossy form cannot be copied
    safely on a decision screen: a CR-only or trailing-CR change would read
    identical to the person approving it.
  - `ctrl-o-arabic-letter-mark` (unsafe-to-copy, owner T-U6b). fx writes
    U+061C raw, and it reorders the text around it as the U+200E/U+200F
    marks fx does escape. Ctrl-O names it, `\u{061c}`. It is resolved when
    fx escapes it, at this pin's line or in a later pin. No scenario holds
    one.
  - `submit-multi-card-whole` is **not** an approved divergence: no user
    decided it, so it approves nothing.
    - `ledger.py` has no `awaiting-decision` class (its classes are
      user-decision, unsafe-to-copy, replaced-in-next-pin and
      platform-limit). The nearest honest form is a `[[cause]]` of kind
      `decision`, which the sweep prints as "a = awaiting a user decision".
    - Its question, for the epic-4 preview: copy fx's one-frame last-line
      preview of a multi-line card on a full screen, or keep the card whole
      from the first frame?
    - The owner is `user decision multi-line card preview`. Until the user
      answers, xfx keeps the whole card, the dispatcher's interim choice.
- **`docs/parity.md`:**
  - the full-screen TUI row: the submit frame, the tall prompt, and the
    review's spelling against fx's;
  - the Ctrl-O row: fx's forms and the one addition; the read, argument and
    command rows stay open as **T-U1-SAFE**;
  - the `edit_file` row: a file that is not UTF-8.
- **Scenarios** (`scripts/fxcap/scenarios/`, from T-U6):
  - `p5-transcript-diffctl`: xfx gets its own turns. xfx's `edit_file`
    refuses a file it has not read in full, so it reads `ctl.txt` first. It
    also gets `XFX_PERMISSION_MODE=yolo`. fx's lines are unchanged.
    **T-U1U2's `regressions.txt` run of this scenario is stale**: it ran
    with fx's turns, under which xfx's edit was refused.
  - New:
    - `p5-transcript-diffodd`: a tab, U+202E, U+200B, U+FEFF, a CRLF line, a
      DEL and a literal `\x1b`;
    - `p5-transcript-diffbad`: a file that is not UTF-8;
    - `p5-transcript-diffask`: `ctl.txt` reviewed in `ask` mode, then
      applied;
    - `p5-transcript-submit-multi`: a three-line prompt on a full screen.
  - Their files are in `p5-transcript-diffws/`.
- **transcript-wrap's `check.py` hardening** (T-U1+U2's review). See that
  README's "Round 4":
  - `record()` pins the footer to row 38;
  - `check_insert` pins the inserted record's row and height;
  - `mutants-c.py` asserts its own outcomes;
  - `README.md:227` is scoped to its exceptions.

## RED

- **T-U6b, the bytes** (`red-u6b.txt`, `check.py … red-u6b`, T-U6's build):
  34 claims, 28 hold, **6 fail**. They are the Ctrl-O `spelled` and `bytes`
  claims of `diffctl`, `diffodd` and `diffask`. For example:
  - `1   : fx '…  │ 1   before\\x1b[2Jcleared…' / red-u6b '…  │ 1   before\\u{001B}[2Jcleared…'`
  - `2   : fx '…  │ 2   tab\x1b[39m     \x1b[38;5;245mhere…' / red-u6b '…  │ 2   tab\\there…'`
- **T-U6, the security claim** (`red.txt`, the base's build): the same 6
  fail.
  - On `diffodd` rows 27-28, Ctrl-O also hands the terminal U+202E, U+200B
    and U+FEFF as themselves.
  - `red-test.txt` is T-U6's first test on `039fb5c8`'s code:
    `'\u{202e}' reached a row`.
- Every T7 and T8 claim holds on both earlier builds: T-U6b changed nothing
  there.

## GREEN (`green.txt`: 34 claims, 34 hold)

`check.py` reads every capture cell by cell, styles included. Where it
compares two sides, it applies the ledger's normalizations first. It sets
nothing else aside unless a claim names it.

### T10 bytes: Ctrl-O's diff rows are fx's (`bytes`)

Each diff row is the same bytes on both sides, as `capture-pane -e` holds
them. On `diffodd` `full`:

```
1 -  '\x1b[38;5;252m  │ 1 - first line\x1b[39m'
1 +  '\x1b[38;5;252m  │ 1 + FIRST LINE\x1b[39m'
2    '\x1b[38;5;245m  │ 2   tab\x1b[39m     \x1b[38;5;245mhere\x1b[39m'
3    '\x1b[38;5;245m  │ 3   bidi \\u{202e} reversed'
4    '  │ 4   zero\\u{200b}width \\u{feff} bom\x1b[39m'
5    '\x1b[38;5;245m  │ 5   crlf line\x1b[39m'
6    '\x1b[38;5;245m  │ 6   del \\x7f and literal \\x1b text\x1b[39m'
```

On `diffctl` `full`, and on `diffask` `full` after the change was applied:

```
1    '\x1b[38;5;245m  │ 1   before\\x1b[2Jcleared\x1b[39m'
2    '\x1b[38;5;245m  │ 2   second \\u{009b}line\x1b[39m'
3 -  '\x1b[38;5;252m  │ 3 - third line\x1b[39m'
3 +  '\x1b[38;5;252m  │ 3 + THIRD LINE\x1b[39m'
```

Row 2's tab is fx's: the cells it moves over are unstyled on both sides.
Rows 3-4 share a colour run, so tmux writes one SGR for the two.

### T10 spelling and safety (`spelled`, `refused`, `nodiff`)

- **`spelled`:** every diff row is the fixture's line N, spelled as the
  claim says, and the rows are exactly the edit's window.
  - Ctrl-O (`diffctl`, `diffodd`, `diffask` `full`): fx's spelling on both
    sides. `fx_spelled` ports `appendTerminalSafeLine`; on xfx's side it
    also names U+061C.
  - The review (`diffask` `ask`): fx's spelling on fx's side, xfx's own on
    xfx's (`review-diff-spelling`). The two rows spelled apart are
    `before\x1b[2Jcleared` / `before\u{001B}[2Jcleared` and
    `second \u{009b}line` / `second \u{009B}line`, with fx's styles.
  - On every snap, no cell of a diff row holds a control or anything fx's
    `isNonPrintingCodepoint` names.
- **`refused` (`diffbad`):**
  - fx edits the file that is not UTF-8 and shows `bad \xff byte`.
  - xfx's call fails (`└ Failed bad.txt: edit_file will not replace …`).
    Its read was `binary or non-utf8 file omitted (34 bytes)`
    (`src/tools/read.rs:1162`), and `edit_file` refuses non-UTF-8 anyway
    (`src/tools/mutate.rs:437-442`).
  - A `write_file` over such a file reviews its before as `<N bytes of
    non-UTF-8 data>` (`mutate.rs:659-661`).
- **`nodiff`, `inline`:**
  - Neither inline transcript draws a diff; each shows `└ Edited <file> +1 /
    -1`.
  - `diffodd` `edited`/`closed` and `diffask` `applied`/`closed` are
    identical on the two sides.
  - Ctrl-O opens and closes on both sides.

### T10 security, from the code

`Review` has three consumers, and they are every diff row xfx draws:
- the band, `src/tui/approval.rs:329`, `:490`;
- the plane, `src/tui/approval_screen.rs:142`, `:337`;
- Ctrl-O, `src/tui/full_transcript.rs` `diff_rows`.

1. **Band and plane:** `Review::rows` → `safe_rows` → `tamed`
   (`approval_view.rs:543`). Unchanged.
2. **Ctrl-O:** `terminal_safe_line(unspelled(…))`. Its output escapes every
   C0, C1, DEL and fx non-printing code point, and U+061C, whatever the
   decoder returns. So a hand-written or tampered session log cannot put a
   raw byte on the screen. The one raw control left, a tab, becomes blank
   cells (`drawn_tabs`).
3. **Inline:** none.

Two mutants check this:
- **`code-mutant.txt`**: a throwaway clone (`git clone --no-local`) whose
  `fx_names` loses its U+2028-202E arm. Two tests fail. One fails with `a
  row hands the terminal a character it acts on: "  │ 1 + bidi \u{202e} x"`;
  the other is the table's `left: "bidi \u{202e} x"`.
- **`mutants.py`**: a raw U+202E in xfx's captured Ctrl-O row fails with
  `the terminal was handed ['U+202E'] as itself`.

### T7: the card is in the frame that clears the composer (70611a91)

- **The frames.** `frames.py` replays each side's wire into a private tmux
  pane up to the end of frames 1 and 2. Its `stty -echo` keeps tmux's
  answers to the wire's own queries off the screen.
  - `replay`: the whole after-wire replayed is the `second` snap taken just
    before that wire was copied, every row, on both sides.
- **`p5-transcript-submit`:**
  - frame 1, on both sides: composer row 38 is `┃` alone, and `┃ second
    prompt` sits one blank row under the previous turn's summary.
  - `same`: aligned at the card, the rows above it and its rows are the
    same, and so are rows 38-40.
  - Set aside and printed: the rows between the card and the composer. That
    is the band's activity row, which fx writes with the card (`• Thinking`)
    and xfx once the runtime says the turn started, in the same frame or
    the next.
  - The 0.15 s snap is compared the same way. `filled` and `second` are
    identical.
  - The xfx path: `submit` → `send` → `echo`, in the `route_bytes` that takes
    the draft (`src/tui/shell.rs:11617`, `:11812`).
- **`p5-transcript-submit-multi`:**
  - fx's frame 1 previews only `┃ multi line three`, and its frame 2 writes
    all three rows.
  - xfx's frame 1 writes all three.
  - aligned at the card, fx's frame 2 and xfx's frame 1 are the same.
  - `filled`, `second` and `second.full` are identical.
  - Ledger: the cause `submit-multi-card-whole` (awaiting the user).

### T8: a tall first prompt reaches scrollback whole (433464dd)

- `p5-transcript-tallfirst` `tall.full`: all 60 card rows appear once each,
  in order, at rows 3-62, under the banner and one blank row.
- With an earlier turn above it (`p5-transcript-submit` `tall.full`): the
  same at rows 73-132, under the previous turn's summary.
- `tall` and `tall.full` are identical on the two sides.

## Findings no claim covers (printed by `check.py`)

**T-U1-SAFE (another worker):** Ctrl-O's read, argument and command rows
(`Detail::made_inert` → `bridge::inert`, then `railed`/`literal`).
- They blank a control to a space: `before [2Jcleared`, where fx shows
  `before\x1b[2Jcleared`.
- They let U+202E, U+200B and U+FEFF cross as themselves (`diffodd` `full`
  rows 16-17).
- That path is left alone here, and kept open in the Ctrl-O row of
  `docs/parity.md`.

## Files

- **`fx/`, `xfx/`, `red-u6b/`, `red/`:** the captures, redacted with
  `scripts/fxcap/redact.py`. The wires are copied as bytes after a
  local-root check. `-frame1`, `-frame2` and `-wireend.ansi` come from
  `frames.py`.
- **The scripts:** `publish.py`, `frames.py`, `check.py`, `mutants.py`.
- **The outputs:**
  - `green.txt`, `red-u6b.txt` and `red.txt`: `check.py`'s output on each
    side;
  - `mutants.txt`: 11 mutants (T7 ×3, T8 ×2, T10 ×6). Each fails with the
    FAIL line it names, the unchanged captures pass, and the run exits 0;
  - `code-mutant.txt`: the bidi code mutant;
  - `red-test.txt`: T-U6's RED test.

Commands, from the worktree (`R=.prd/tui-phase5/receipts/transcript-receipts`):

```
python3 -I -B $R/publish.py . <scratch> $R f84734d 039fb5c 4aca257
python3 -I -B $R/frames.py $R /private/tmp/claude-501/tr/fr
python3 -I -B $R/check.py . $R xfx       # green.txt
python3 -I -B $R/check.py . $R red-u6b   # red-u6b.txt
python3 -I -B $R/check.py . $R red       # red.txt
python3 -I -B $R/mutants.py . $R /private/tmp/claude-501/tr/mu
```

## Gates

These ran on `f84734dc`, the code under this receipt; this receipt's commit
adds only the receipt. `TMPDIR` was `/private/tmp/claude-501/tr`, and each
suite ran alone, after the idle wait.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4928 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4957 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh` | ok, ok |
| `scripts/check-no-secrets.sh`, run again once this receipt was staged whole | ok |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | OK, both suites |
| `check.py … xfx` (`green.txt`) | 34 claims, 34 hold |
| `check.py … red-u6b` (`red-u6b.txt`) | 34 claims, 28 hold, 6 fail (Ctrl-O `spelled`, `bytes`) |
| `check.py … red` (`red.txt`) | 34 claims, 28 hold, 6 fail (the same, with raw U+202E/U+200B/U+FEFF) |
| `mutants.py` (`mutants.txt`) | 11 mutants, each failed with its line; exit 0 |
| `code-mutant.txt` | the bidi arm dropped: 2 tests fail |
| transcript-wrap `check.py`, `mutants-c.py` | unchanged by T-U6b (34/34; 9 mutants held) |
