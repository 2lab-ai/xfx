# META-PREFIX-DECODE receipt: fx's meta prefix and every modifier bit, a person's two Escapes kept

fx v0.0.16 reads a second `ESC` that comes before the first has gone quiet
as a **meta prefix**: no event, and the key behind it is read as that key
with Alt. With nothing behind it the pair is two Escapes, as two presses
are. It also reads a legacy cursor-key modifier field for its four bits,
whatever else the field holds. xfx's decoder read `ESC ESC` and a key as the
Escape key and then the key, so `ESC ESC [ A` cancelled a turn at an
approval and recalled a prompt over a running turn. It read a field past 16
as no key. This unit copies both rules into the decoder (`src/tui/input.rs`).
A person's two presses, and two `ESC` bytes in one burst, stay two Escapes.

| commit | what |
|---|---|
| `c76e5a51` | RED: the decoder's generated table over every mask, prefixed and not; three new decoder tests; three shell tests (an idle draft, a menu, a running turn); the `1;17C` pin moved to `1;257C`. Scenarios `p5-cancel-double-escape`, `p5-cancel-double-escape-turn`, `p5-modified-meta-prefix`. Tests and scenarios only. **Its burst rows were wrong** (below) |
| `adb740bd` | RED: a shell test at an approval's choice and a question's. Test only |
| `d83ef4eb` | the stages' `meta`, `after_escape`, `in_csi`, `csi`, `prefixed`, `ss3`, `kitty_key`, `modifier`, the module doc |
| `2fbe3e83`, `fbb2a50f` | `docs/parity.md`, the ledger's refs, a scenario comment; citations named in comments |
| `47fccd97` | **the code head**: a quiet prefix flushes as two Escapes, correcting `c76e5a51` and `d83ef4eb`; the burst rows, the shell pins, the scenarios and the parity sentence to match |
| the commits after | this folder; no `src/` change |

**Corrected within the unit.** `c76e5a51`'s burst rows, and `d83ef4eb`'s
flush, read a lone `ESC ESC` as one Escape: fx's key decoder alone resolves a
prefixed lone `ESC` as one (`escape_parser.zig:327-329`). fx's first
captures of `p5-cancel-double-escape` and `-turn` showed two: the burst
cleared a draft and cancelled a turn. The theme monitor is in front of the
key decoder at every launch, and it splits the pair (below). `47fccd97`
made the flush give two, and kept every prefixed key as it was: a key behind
the pair still reaches the decoder as `ESC ESC` and the key. `red.txt` has
the corrected tests over the base first, and the first runs after them.

The source change outside the test modules is +195/-77, all in
`src/tui/input.rs` (`srcsize.txt`, `srcsize.py`: the hunks above the file's
`mod tests` in `git diff -U0 adb740bd 47fccd97 -- src`). That is under the
brief's 300.

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**How the prefix is detected.**
- The stage is one byte, and the prefix is its top bit
  (`escape_meta_mask`, `escape_parser.zig:25`; `hasMetaPrefix`,
  `:287-289`).
- An `ESC` in the bare stage sets it and returns nothing
  (`:552-555`). The decoder around it then returns no event and starts
  the clock again from that byte (`terminal_action_decoder.zig:84-88`).
- A third `ESC` is the same arm again: the bit stays.
- An `ESC` in any other pending stage drops the bit and starts a bare
  stage (`escape_parser.zig:492-498`).
- `[` and `O` move to the CSI and SS3 stages and keep the bit
  (`setBaseEscapeStage`, `:291-293`; `:502-514`); so do the CSI's later
  stages.

**What each prefixed form decodes to.**
- **Arrows**: the bare CSI and SS3 stages give Up, Down, Left and Right
  `modifiedArrowAction(byte, 0, true)` (`:561-568`, `:637-644`). The
  modifier stage gives `modifiedArrowAction(byte, modifiers, meta)`, and
  the plain key where that is `null` (`:721-737`).
  `modifiedArrowAction` reads the prefix once, beside Alt's bit (`:83`).
  So a prefixed arrow is the arrow with Alt: Up `paragraph_up`, Down
  `paragraph_down`, Left `word_left`, Right `word_right` (`:83-89`), and
  Super is still read first (`:74-82`).
- **Home and End**: the plain key, bare (`:568`, `:644`) or held. Alt's
  arm has no Home or End row and answers `null` (`:83-89`, `:727-733`).
  So `ESC ESC [ 1;5H` is `.home`, and `ESC ESC [ 1;9D` is Super's
  `line_start` (`:78`).
- **PgUp, PgDn and the tilde keys**: neither tilde stage reads the bit
  (`:620-636`, `:694-719`). `ESC ESC [ 5 ~` is `.page_up`, and
  `ESC ESC [ 200 ~` opens a paste (`:625`).
- **Shift-Tab** reads no bit (`:569`, `:723-725`).
- **Kitty `CSI u`** and the modifyOtherKeys `~` hand the bit to
  `kittyUnicodeKeyAction` (`:613-619`, `:686-692`, `:746-752`,
  `:768-778`), which reads it in six places:
  - a keypad digit is not typed (`:195`), and nothing after reads its
    code (`:265`);
  - a keypad arrow, Home or End goes through `navigationKeyAction`
    (`:163-167`, `:197`);
  - kitty's own Up and Down take the arrow table, and `.ignore` where it
    answers `null` (`:208-217`). So Hyper alone is `.ignore` bare and a
    paragraph prefixed;
  - Enter with Ctrl skips its first row (`:218`) and is still Enter,
    through the bare one (`:262`);
  - Shift+Space is not a space (`:222`);
  - `d` deletes the word on its right whatever else is held (`:251`), and
    `o` is nothing (`ctrlOKeyAction`, `:111-114`, `:252`).

  Escape (`:188`), the page keys and Delete (`:169-180`, `:198-205`) read
  no bit. `b` with nothing held is no Alt-b (`:243`).
- **A prefixed printable** (`ESC ESC x`): the bare stage's own bytes read
  alike behind the prefix. `b`, `f`, `d`, `DEL`/`BS`, `CR`/`LF` are Alt-b,
  Alt-f, Alt-d, Alt-Backspace and Alt-Enter (`:515-551`). Every other byte
  is `.ignore` and goes with it (`:556-559`).
- **A prefixed control byte** is that `.ignore` too. The replay that
  gives a bare `ESC`'s control byte back runs only when the stage before
  it was exactly `1` (`terminal_action_decoder.zig:92-101`), and the
  prefixed stage is `0x81`.
- **A prefixed `ESC`** keeps the prefix (`escape_parser.zig:552-555`,
  above).

**How a person's double Escape, and an `ESC` at the end of a read, are
told apart from a prefix: by the quiet alone, in two places.** fx's theme
monitor is started at every launch, under `FX_THEME` too
(`app_bootstrap_runtime.zig:481-483`), and it sees every byte before the
key decoder does (`terminalInputOwner`, `src/ui/input/runtime.zig:46-55`;
`main.zig:3127-3134`).
- **The monitor holds a lone `ESC`** as the start of a report it may own,
  with a 75 ms deadline (`theme_monitor.zig:5-7`, `:84`, `:91-93`, `:106-110`).
- **A second `ESC` while it holds one goes to the decoder at once**, and
  the held one stays held, its deadline started again (`:85-88`). So does
  every further `ESC`.
- **A byte that is not `ESC` joins the held one**, and once the two are no
  report's start they go to the decoder together, the held `ESC` in front
  (`forwardCandidate`, `:147`, `:271-278`). So the decoder reads
  `ESC ESC [ A` as those four bytes, in that order: one key.
- **With nothing behind it, the held `ESC` goes to the decoder on its own
  once its deadline passes** (`poll`, `:150-164`; `deferCandidate`,
  `:261-269`; `collectThemeFacts`, `main.zig:2804-2806`), through the
  loop's collected bytes (`runtime.zig:685-701`, `main.zig:3160-3163`,
  `event_loop.zig:83-86`).
- **The decoder** resolves a stage older than `input_escape_timeout_ms`,
  30 ms (`main.zig:187`; `terminal_action_decoder.zig:135-139`). A
  prefixed lone `ESC` is still the bare stage (`isBareEscapeStage`,
  `escape_parser.zig:327-329`), and its flush emits exactly one `.escape`
  (`terminal_action_decoder.zig:174-183`). The flush runs once a loop turn,
  before the read (`main.zig:2975-2982`; `event_loop.zig:79-92`), and the
  poll waits 8 ms at most while work is active (`main.zig:174-176`). No
  rule reads which read a byte came off: a read's bytes are fed one at a
  time (`event_loop.zig:94-107`).
- So **an `ESC` at the end of a read is held 75 ms by the monitor, then 30
  ms by the decoder, before it is the Escape key.** Two `ESC` bytes in one
  burst, with nothing behind them, are **two** Escapes: the decoder's for
  the second (and any after it) at about +30 ms, and the held one's at
  about +105 ms. A person's two presses are two as well, however far apart.
  fx's captures show the burst's two: `fx/...-b-burst-cleared` (a draft
  cleared) and `fx/...-turn-...-b-burst-cancelled` (a turn cancelled).
- fx's own tests pin the decoder's prefix (`src/ui/input/runtime.zig:2266-2284`,
  `:3585-3604`); they feed the decoder alone, which is why `c76e5a51` read
  one Escape there.

**A report behind a second `ESC`.** The held `ESC` and the report's bytes
are matched together, and the monitor keeps the background notification
and an `OSC 11` reply (`theme_monitor.zig:96-105`, `:123-146`). The second
`ESC` went to the decoder on its own, so the user's Escape in front of a
report is the Escape key.

**The modifier field's bits, on every legacy CSI form.**
- The field is a number that stops at 65535 (`appendCsiDigitSaturating`,
  `escape_parser.zig:19-23`; `:662-666`), and the modifiers are one less
  (`:722`).
- The arrow table reads four bits: Shift, Alt, Ctrl and Super
  (`:61-64`, `:73-108`). Hyper (16), Meta's bit (32), Caps Lock (64) and
  Num Lock (128) are read nowhere on these forms. So `1;17C` is Right,
  `1;33H` Home, `1;69H` (Ctrl and Caps Lock) `draft_start`, and `1;129D`
  Left.
- The tilde keys read every bit as "a modifier" (`:699-711`), and Delete
  every bit (`:176-180`, `:698`); both were already copied
  (DECODER-TILDE-HOME-END, TILDE-EMPTY-MODIFIER).
- Shift-Tab is any modifier at all (`:723-725`).
- A kitty report masks the lock bits off (`:186-187`), and Hyper or Meta's
  bit keeps a keypad key from being the keypad (`:189-191`); already
  copied.

**What an Escape does, by surface** (`resolveEscape`,
`app_input_runtime.zig:2687-2800`; the windows `gesture_state.zig:1-6`).
- Over running work the first press arms the interrupt, and a second
  within 1 s stops it (`app_input_runtime.zig:2740-2758`).
- At an approval it cancels (`app_input_runtime.zig:2712-2717`).
- A menu or an inline picker closes, and the clear is disarmed
  (`app_input_runtime.zig:2718-2729`, `:2765-2779`).
- At an idle draft the first press arms the clear, and a second within
  500 ms clears the draft (`app_input_runtime.zig:2784-2799`).
- A prefixed key that is not an Escape stands an armed interrupt down, as
  any other key does (`app_input_runtime.zig:906-913`).

## What xfx does now (`src/tui/input.rs`)

- `Stage::Esc`, `Stage::Csi` and `Stage::Ss3` carry `meta`.
- `after_escape`: an `ESC` sets `meta`, with no event and the clock
  started again. `[` and `O` carry it. A control byte behind it is one
  `Ignore`.
- `flush`: a lone `ESC` is one Escape, and a quiet prefix two, as fx's
  monitor and decoder together deliver them.
- The cursor keys read the prefix as Alt's bit (`prefixed`), through
  `cursor`, in the CSI and SS3 forms and on the keypad's arrows. That
  transcription is exact: fx's `modifiedArrowAction(b, m, true)` equals
  `modifiedArrowAction(b, m | 2, false)` for every letter and mask, since
  the prefix is read only in Alt's condition (`escape_parser.zig:83`) and
  `composerMove` reads only Shift (`escape_parser.zig:66-71`). The
  generated table checks it: 9,240 decodes, every mask with and without the
  prefix on every form.
- `kitty_key` reads `meta` in exactly fx's six places, not as Alt's bit.
  An Alt-bit shortcut would make `ESC ESC [ 98 u` Alt-b and `ESC ESC [ 13 u`
  a newline (mutant M15).
- `modifier` reads a field of up to three digits, `1` to `256`.
- A report behind the prefix gives the first `ESC` back as the Escape key:
  an `OSC`, or the background notification `CSI ? 997 ; N n`.
- The tilde keys, the paste markers and Shift-Tab are unchanged, as fx
  treats them alike.

**Unchanged readers.** No surface reads a byte. Every reader takes the
decoded action, and a prefixed key's action is the one its Alt spelling
already had (`ParagraphUp`, `WordLeft`, `Home`, `LineStart`, ...), which
earlier units ported surface by surface. The shell, the question, the
approval, the full transcript and the menus are unchanged.

## The oracle (`fx/`, `captures.txt`)

Three scenarios, captured on fx, the base and the code head in one run
under the suite lock:
- `p5-cancel-double-escape` (6 snaps): at an idle draft, a person's two
  presses (a) and a burst of two (b); `/he`'s menu, a person's three
  presses (c) and a burst of two then one press (d).
- `p5-cancel-double-escape-turn` (5 snaps): over a held answer,
  `ESC ESC [ A` (a) and a burst of two (b); over the next held answer, a
  person's two presses (c).
- `p5-modified-meta-prefix` (14 snaps): the prefixed Up, Down, Left, Right,
  Ctrl-Home, Super-Left and SS3 Up, each followed by a letter; then Ctrl
  with Caps Lock on Home and End, Ctrl with Num Lock, Meta's bit alone,
  Ctrl with Meta's bit and Hyper alone.

What fx shows:
- **The burst is two Escapes.** `b-burst-cleared`: the draft is gone.
  `d-burst-cleared`: the menu closed and the press after it cleared the
  draft. `-turn`'s `b-burst-cancelled`: `▪ Cancelled`.
- **A person's presses** clear the draft (`a-person-cleared`), close the
  menu and then clear (`c-person-closed`, `c-person-cleared`), and cancel the
  second turn (`c-person-cancelled`).
- **`ESC ESC [ A` over a turn** leaves it running with the composer empty
  (`a-meta-up`: `• Generating`).
- **In the composer**: a paragraph up to the paragraph's start (`Afive
  six`), down to its end (`B`), a word left and right (`C`, `D`), the plain
  Home for the prefixed Ctrl-Home (`E`, the line's start), the line's start
  for the prefixed Super-Left (`F`) and SS3's Up (`G`); Ctrl and Caps Lock's
  Home and End the draft's edges (`H`, `I`); Ctrl and Num Lock's Home the
  draft's start (`J`); Meta's bit alone End (`K`); Ctrl with Meta's bit a
  word left (`L`); Hyper alone Left (`M`).

## RED (`red.txt`, `red/`, `red-pairs.txt`)

- `red.txt` 1: the corrected tests (`47fccd97`'s) over the base's code. Six
  of ten fail:
  - the decoder: `ESC ESC b` gave `[Escape, AltB]` for `[AltB]`; `ESC ESC
    [ A` `[Escape, Up]` for `[ParagraphUp]`; `1;17A` `Ignore` for `Up` (the
    table's first mask past 16); the burst resolved before the quiet;
  - the shell: over a running turn `ESC ESC [ A` recalled `stream lines`
    into the composer; at an approval's choice it cancelled the turn
    (`Some(Cancel { through: 1 })`).
- Four pass on the base, and are pins: the report behind a prefix (the
  base already read `ESC` and then the report), the exact-shapes test, and
  the two shell burst tests (the base read a burst as two Escapes too).
- `red.txt` 2 keeps the first runs, before each RED commit, with
  `c76e5a51`'s wrong burst rows.
- `red-pairs.txt` (`pairs.sh red`, banner `v0.1.0-df45025`): 7 of 25 pairs
  identical.
  - Identical: every burst and person snap at the idle draft and the menu
    but `c-menu`, `-turn`'s `mid`, the composer's `draft`.
  - Different: over the turn `ESC ESC [ A` put `stream please` in the
    composer (`a-meta-up`), and every turn snap after it carries it; every
    composer snap after the draft, the letters landing where Escape and
    the plain key or nothing put the caret.

## GREEN (`green-pairs.txt`, `green.txt`, `xfx/`)

- `green-pairs.txt` (`pairs.sh xfx`, banner `v0.1.0-47fccd9`): 20 of 25
  pairs identical, every composer snap among them. The other five differ
  in rows this unit does not draw, the same rows on the base:
  - `c-menu`, rows 5, 7 and 8: fx's `/he` lists `Commands 2`, `/help` and
    `/shell`, and xfx has no `/shell`;
  - `-turn`'s `b-burst-cancelled`, `c-person-cancelled` and `mid2`, row 5
    (and 9): `▪ Cancelled · What can fx do differently?`, the product's
    name, which the sweep's `product-name` normalization reads as xfx's;
  - `-turn`'s `a-meta-up`, row 5: fx's `•` in front of `Generating`, where
    xfx's row has the blank. The marker blinks (`docs/parity.md`, "The
    marker blinks every 500 ms"), so the two snaps caught it in different
    phases [inferred from the one cell; `mid`'s pair is identical].
- `green.txt`: the unit's 10 tests at the code head, on both suites:
  10 + 10 ok.

## Mutants (`mutants.txt`, `mutants.py`)

On a `--no-local` clone at `47fccd97` under the suite lock, one mutant at a
time, each running `cargo test --lib -- tui::`: **22 of 22 killed**, the
clone clean after the run.

| mutant | killed by |
|---|---|
| M1 the second ESC the Escape key and a new ESC (the base) | the prefixed keys, the burst, the prefixed stage, the approval and question test (+more) |
| M2 a third ESC emits an Escape | the prefixed keys (`ESC ESC ESC [ A`), the burst |
| M3 the prefix's clock left at the first ESC | the burst (its clock case) |
| M4 the prefix dropped at `[` | the prefixed keys, the report test, the generated table (+more) |
| M5 the prefix dropped at `O` | the prefixed keys, the table, the approval and question test |
| M6 the cursor table ignores the prefix | the prefixed keys, the table, the prefixed stage |
| M7 SS3 ignores the prefix | the prefixed keys, the table, the approval and question test |
| M8 a keypad arrow ignores the prefix | the table |
| M9 kitty's Up and Down ignore the prefix | the table, the prefixed stage |
| M10 kitty's Up with Hyper alone nothing behind the prefix | the table, the prefixed stage |
| M11 a prefixed keypad digit typed | the prefixed stage |
| M12 a prefixed Shift+Space a space | the prefixed stage |
| M13 a prefixed `d` not the word on the right | the prefixed stage |
| M14 a prefixed Ctrl+O opens the full transcript | the prefixed stage |
| M15 a key report reads the prefix as Alt's bit | the prefixed stage |
| M16 a control behind the prefix replayed after an Escape | the prefixed stage |
| M17 an `OSC` behind the prefix loses the Escape | the report test |
| M18 the background report behind the prefix loses the Escape | the report test |
| M19 the modifier field stops at 16 | the prefixed keys, the table |
| M20 the modifier field takes two digits | the prefixed keys, the table |
| M21 a field past 256 read for its low byte | the exact-shapes test, the table |
| M22 a quiet prefix one Escape (fx's decoder alone, `c76e5a51`'s reading) | the burst, the running-turn shell test |

M8's list also names `tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_o...`,
a catalog-wait test (the known flake class); the table kills M8 on its own.
The first run, at `2fbe3e83` (before the correction), killed its 21 of 21
too; it is not kept, since its code is not the head's.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The command, at 40x120 with `--jobs 2`, on this branch's scenarios and
ledger:

```
scripts/fxcap/sweep.sh --only 'composer*' --only 'p5-modified-*'
  --only 'p5-paragraph-*' --only 'p5-super-arrows*' --only 'p5-input-*'
  --only 'p5-page-keys*' --only 'cancel*' --only 'p5-cancel*'
```

It ran once with the base's binary (`v0.1.0-df45025`) and once with the
code head's (`v0.1.0-47fccd9`), both in one suite-lock slot, at load 27 to
48. That is 37 scenarios and 317 pairs; `p5-page-keys*` and `cancel*` match
no scenario (the page keys are `p5-modified-page-keys`, in
`p5-modified-*`). The oracle's sha256 was `8e6cda2e…` before and after
(`sweep-compare.txt`'s header). APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py`
(unchanged, run from its folder) compares them pair by pair:

- **base 238 / 317, head 255 / 317, drops 0.** No scenario failed or timed
  out on either side.
- **The 17 pairs that change to a pass are this unit's**: the composer's 13
  prefixed and lock-bit snaps (all but `draft`), and the turn's four after
  `mid` (`a-meta-up`, `b-burst-cancelled`, `mid2`, `c-person-cancelled`).
  On the base, `ESC ESC [ A` put the last prompt back in the composer, and
  every snap after it carried it.
- **This unit's three scenarios** pass 24 of 25 at the head. The 25th,
  `c-menu`, is unresolved on both sides under the ledger's existing causes
  (`menu-count`, `menu-shift`, `C8-shell`: fx lists `/shell`, which xfx
  does not have), with no uncaused row.
- **One other pair changes class**, `p5-input-question20/q20-answered`,
  from normalized-identical to identical: a pass both ways. It is the
  same pair COMPOSER-SUPER-ARROWS' sweep saw change, through the
  `turn-wall-time` normalization of its turn's seconds.
- **Uncaused rows**: 27 pairs have them at the base and 10 at the head.
  All ten have the same rows at the base: `p5-input-approval`'s `after`,
  `cmd-ask` and `edit-ask`; `p5-input-paste-amend`'s `end`;
  `p5-modified-page-keys`' `full-pgup`; `p5-paragraph-move`'s four
  full-transcript screens; `p5-super-arrows-approval`'s
  `amend-super-right` (its trimmed caret blank). All are older than this
  unit.
- **A person's double Escape**: `p5-cancel-double-escape`'s
  `a-person-cleared`, `c-person-closed` and `c-person-cleared` pass on both
  sides, and so do its bursts (`b-burst-cleared`, `d-burst-cleared`).
  `-turn`'s `c-person-cancelled` passes at the head only: on the base its
  composer still held the prompt `a-meta-up` recalled, and its two presses
  cancelled the turn on both sides (`red-pairs.txt`).

## Gate

GATE: dispatcher

## Refs (`refs.txt`, `refs.py`)

`refs.py` is COMPOSER-SUPER-ARROWS', with its paragraph re-pointed at this
unit. It checks the ledger's refs into the files this unit changes, by
content, against the base (`refs.py df45025c`). 6 of 16 had moved (the new
shell tests sit above them), and `--fix` re-pointed them (`2fbe3e83`);
`refs.txt` reads 16 checked, 0 wrong.

## Parity (`docs/parity.md`)

- **The full-screen TUI row, the decoder sentence**: after "a lone `ESC`
  resolved as the Escape key only after 50 ms of quiet" it now states the
  copied rule. A second `ESC` before that quiet is fx's meta prefix, and
  the key behind it is that key with Alt (the cursor keys, a key report
  where `kittyUnicodeKeyAction` reads it, the tilde keys, the paste markers
  and Shift-Tab without it). A control byte behind it is one keystroke with
  no binding. A report behind it gives the first `ESC` back as the Escape
  key. The pair with nothing behind it is two Escapes, as fx's theme
  monitor delivers it. It names the one part **not copied**: the lengths of
  the quiet (fx's monitor 75 ms, then its decoder 30 ms; here one 50 ms).
- **The same row, the cursor-key grammar sentence**: a modifier field is
  read for every bit a terminal sends, `1` to `256`, its four bits only;
  any modifier is Shift-Tab; a field past 256 is no key, where fx reads
  the low bits, as kitty's field is.
- **The `ask_user_question` row** and **the approval's "Not changed"
  sentence**: both said `ESC ESC [ D` (and `[ A`/`[ B` at a choice)
  cancelled the batch or the turn, owner META-PREFIX-DECODE (proposed). Both
  now say the prefixed arrows leave the question and the choice, as fx's
  do. **META-PREFIX-DECODE is no longer an owner anywhere**: `grep -c
  'META-PREFIX-DECODE\*\* (proposed\|owner \*\*META-PREFIX-DECODE'
  docs/parity.md` prints 0.
- `scripts/fxcap/scenarios/p5-approval-vertical-keys.cap`'s note about
  `ESC ESC [ A` names the test that now pins it.

## Decisions

- **A quiet prefix is two Escapes, not one** (`47fccd97`). fx's key decoder
  alone gives one, and its tests feed the decoder alone. The fx binary gives
  two, because its theme monitor sits in front at every launch. Copied the
  binary's behaviour: the user sees the binary, and the capture is the
  oracle. This also keeps what the base did for a burst, so a person's
  double Escape is unchanged in every timing.
- **The quiet stays 50 ms** (`Decoder::ESC_TIMEOUT`). fx's lone Escape
  lands after about 105 ms (75 + 30). Copying that would slow every
  Escape xfx resolves, and change a bound pty tests outside this unit rest
  on. It is recorded as not copied in the parity row.
- **A report behind the prefix gives the first `ESC` back** (an `OSC`, the
  background notification). fx's monitor keeps the report and hands the
  decoder a lone `ESC`, so the user's Escape survives there. Reading the
  report as a prefixed key would lose it.
- **A control byte behind the prefix is one `Ignore`**, as fx's decoder
  reads `ESC ESC` and a control (`terminal_action_decoder.zig:92-101`).
- **A modifier field stops at 256.** fx reads any number and uses the low
  bits. 256 is every bit a terminal sends (kitty's eight), and the
  cursor-key grammar is already stricter than fx's, one canonical spelling
  per field, as `key_modifiers` is for kitty's field.
- **The prefix is Alt's bit for the cursor keys only** (`prefixed`). For a
  key report it is read branch by branch, as fx reads it. Mutant M15 shows
  that an Alt-bit shortcut there would make `ESC ESC [ 98 u` Alt-b.
- **The burst shell tests are pins, not RED.** The base already read a
  burst as two Escapes; they guard the change from turning it into one
  (mutant M22).

## Open

- **The `ESC`-to-`ESC` gap at which fx's monitor still splits a pair** is
  75 ms by its code (`theme_monitor.zig:85-88`). xfx's is 50 ms. Two
  presses 50 to 75 ms apart are two Escapes on both: here because the first
  has gone quiet, in fx because the monitor still holds it and hands the
  second on. Not measured on the binary.
- **A control byte behind the prefix**: fx's monitor hands `ESC` and the
  control on together, so its decoder sees `ESC ESC ^C` and ignores it.
  This is read from the code [inferred], not captured.
- **`a-meta-up`'s one-cell difference** (fx's `•` in front of
  `Generating`) is read as the marker's blink phase [inferred]; not
  reproduced.
