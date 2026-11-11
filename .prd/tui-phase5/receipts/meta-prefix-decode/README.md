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
- The cursor keys read the prefix as Alt's bit (`prefixed`), through
  `cursor`, in the CSI and SS3 forms and on the keypad's arrows. That
  transcription is exact: fx's `modifiedArrowAction(b, m, true)` equals
  `modifiedArrowAction(b, m | 2, false)` for every letter and mask, since
  the prefix is read only in Alt's condition (`:83`) and `composerMove`
  reads only Shift (`:66-71`). The generated table checks it over all
  4 x 256 cases per letter.
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

TODO: the oracle, RED, GREEN, mutants, sweep, gate, decisions, open.
