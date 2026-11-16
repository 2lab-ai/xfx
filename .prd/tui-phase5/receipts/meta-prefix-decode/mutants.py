"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt> [M<n>...]
Applies each mutant to the clone alone, runs the TUI's unit tests there
(`cargo test --lib -- tui::`) under a per-mutant timeout, and restores the
files. A mutant is killed when a test fails. Never run against the worktree:
the clone must sit under `$TMPDIR/mut/`, and its build and temp files go
beside it there. COMPOSER-SUPER-ARROWS' runner, with this unit's mutants:
the second ESC an Escape again, or a third one, or the clock left at the
first (M1-M3), or a quiet prefix one Escape, as fx's decoder alone would
give it (M22); round 2: the prefixed C0 arm leaving the stage pending
(M23, review S1), a leading zero refused in a cursor key's modifier
(M24), the question's choice moving on a prefixed Down (M25); round 3:
the pair's 30 ms stage deleted (round 2's flush), set to 50 ms, emitting
two Escapes at 30 ms, or dropping the kept ESC (M26-M29); the prefix dropped at `[`, at `O`, in the cursor table, in
SS3, in the keypad and kitty arrows (M4-M9); the key report's prefixed
branches undone one at a time, and the prefix read as Alt's bit there
(M10-M15); the control replayed behind the prefix, and a report losing the
Escape it gives back (M16-M18); the modifier field back to 16, to two
digits, and past 256 read for its low bits (M19-M21). Mutant names after
<out.txt> run those alone."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
Q = "src/tui/question.rs"
C0_GROUND = ("            0x00..=0x1f if meta => {\n"
             "                self.stage = Stage::Ground;\n"
             "                out.push(Input::Action(Action::Ignore));\n")
Q_DOWN = "            Action::Down => Some(Self::Down),\n"
SECOND = ("            0x1b => {\n                self.stage = Stage::Esc {\n"
          "                    at: now,\n                    meta: true,\n                }\n            }\n")
CSI = "                self.stage = Stage::Csi { meta };\n"
SS3 = "            b'O' => self.stage = Stage::Ss3 { meta },\n"
PREFIXED = "    if meta {\n        mask | ALT\n    } else {\n        mask\n    }\n"
SS3FN = "        b'A' | b'B' | b'C' | b'D' | b'H' | b'F' => cursor(byte, prefixed(0, meta)),\n"
KEYPAD = ("        } else if let Some(letter) = keypad_navigation(code) {\n"
          "            return Input::Action(cursor(letter, prefixed(mods, meta)));\n")
ARROW_IGNORE = "        if !meta && mods != 0 && mods & (SHIFT | ALT | CTRL | SUPER) == 0 {\n"
ARROW = ("            return ignore;\n        }\n"
         "        return Input::Action(cursor(letter, prefixed(mods, meta)));\n")
DIGIT = "            if !meta && mods & !SHIFT == 0 {\n"
SPACE = "    if code == 32 && mods == SHIFT && !meta {\n"
ENTER = "    if code == 13 && mods == CTRL && !meta {\n"
D = "    if is(b'd') && (meta || mods & ALT != 0) {\n"
O = "        return Input::Action(if mods == CTRL && !meta {\n"
MASK = "    let ignore = Input::Action(Action::Ignore);\n    // Caps Lock and Num Lock are states, not modifiers (:186-187).\n    let mods = modifiers & 0x3f;\n"
C0 = ("            0x00..=0x1f if meta => {\n"
      "                self.stage = Stage::Ground;\n"
      "                out.push(Input::Action(Action::Ignore));\n"
      "            }\n")
OSC = ("            b']' => {\n"
       "                if meta {\n"
       "                    out.push(Input::Action(Action::Escape));\n"
       "                }\n")
THEME = ("                        if meta {\n"
         "                            out.push(Input::Action(Action::Escape));\n"
         "                        }\n"
         "                        out.push(Input::Report(mode));\n")
FIELD = ("        .fold(0u16, |value, digit| value * 10 + u16::from(digit - b'0'));\n"
         "    u8::try_from(value.checked_sub(1)?).ok()\n}\n")
PAIR = ("        if meta && quiet >= Self::PAIR_TIMEOUT {\n"
        "            out.push(Input::Action(Action::Escape));\n"
        "            self.stage = Stage::Esc { at, meta: false };\n"
        "        }\n")
LONE = ("        if quiet >= Self::ESC_TIMEOUT {\n"
        "            self.stage = Stage::Ground;\n"
        "            out.push(Input::Action(Action::Escape));\n"
        "        }\n")
PAIR_CONST = "    pub(crate) const PAIR_TIMEOUT: Duration = Duration::from_millis(30);\n"
DIGITS = "    if digits.is_empty() || digits.len() > 3 || !digits.iter().all(u8::is_ascii_digit) {\n"
MUTANTS = [
    ("M1 the second ESC is the Escape key and a new ESC (the base)", [(I, SECOND, "")]),
    ("M2 a third ESC emits the Escape the first two were", [(I, SECOND,
     "            0x1b => {\n                if meta {\n                    out.push(Input::Action(Action::Escape));\n"
     "                }\n                self.stage = Stage::Esc { at: now, meta: true }\n            }\n")]),
    ("M3 the prefix's clock is left at the first ESC", [(I, SECOND,
     "            0x1b => {\n                let at = match self.stage {\n                    Stage::Esc { at, .. } => at,\n"
     "                    _ => now,\n                };\n                self.stage = Stage::Esc { at, meta: true }\n            }\n")]),
    ("M4 the prefix is dropped at `[`", [(I, CSI, "                self.stage = Stage::Csi { meta: false };\n")]),
    ("M5 the prefix is dropped at `O`", [(I, SS3, "            b'O' => self.stage = Stage::Ss3 { meta: false },\n")]),
    ("M6 the cursor table ignores the prefix", [(I, PREFIXED, "    let _ = meta;\n    mask\n")]),
    ("M7 SS3 ignores the prefix", [(I, SS3FN, "        b'A' | b'B' | b'C' | b'D' | b'H' | b'F' => cursor(byte, prefixed(0, false)),\n")]),
    ("M8 a keypad arrow ignores the prefix", [(I, KEYPAD,
     "        } else if let Some(letter) = keypad_navigation(code) {\n            return Input::Action(cursor(letter, mods));\n")]),
    ("M9 kitty's Up and Down ignore the prefix", [(I, ARROW,
     "            return ignore;\n        }\n        return Input::Action(cursor(letter, mods));\n")]),
    ("M10 kitty's Up with Hyper alone is nothing behind the prefix too", [(I, ARROW_IGNORE,
     "        if mods != 0 && mods & (SHIFT | ALT | CTRL | SUPER) == 0 {\n")]),
    ("M11 a prefixed keypad digit is typed", [(I, DIGIT, "            if mods & !SHIFT == 0 {\n")]),
    ("M12 a prefixed Shift+Space is a space", [(I, SPACE, "    if code == 32 && mods == SHIFT {\n")]),
    ("M13 a prefixed `d` is not the word on the right", [(I, D, "    if is(b'd') && mods & ALT != 0 {\n")]),
    ("M14 a prefixed Ctrl+O opens the full transcript", [(I, O, "        return Input::Action(if mods == CTRL {\n")]),
    ("M15 a key report reads the prefix as Alt's bit", [(I, MASK,
     "    let ignore = Input::Action(Action::Ignore);\n    let mods = (modifiers & 0x3f) | if meta { ALT } else { 0 };\n"), (I, ENTER, "    if code == 13 && mods == CTRL {\n")]),
    ("M16 a control behind the prefix is replayed after an Escape", [(I, C0, "")]),
    ("M17 an OSC behind the prefix loses the Escape", [(I, OSC, "            b']' => {\n")]),
    ("M18 the background report behind the prefix loses the Escape", [(I, THEME, "                        out.push(Input::Report(mode));\n")]),
    ("M19 the modifier field stops at 16 again", [(I, FIELD,
     "        .fold(0u16, |value, digit| value * 10 + u16::from(digit - b'0'));\n"
     "    u8::try_from(value.checked_sub(1)?).ok().filter(|mask| *mask < 16)\n}\n")]),
    ("M20 the modifier field takes two digits", [(I, DIGITS,
     "    if digits.is_empty() || digits.len() > 2 || !digits.iter().all(u8::is_ascii_digit) {\n")]),
    ("M21 a field past 256 is read for its low byte", [(I, FIELD,
     "        .fold(0u16, |value, digit| value * 10 + u16::from(digit - b'0'));\n"
     "    value.checked_sub(1).map(|mask| mask.to_le_bytes()[0])\n}\n")]),
    ("M22 a quiet prefix is one Escape, at 50 ms (fx's decoder alone; the pair's stage deleted)", [(I, PAIR, "")]),
    ("M23 the prefixed C0 arm leaves the stage pending (no Ground reset)", [(I, C0_GROUND,
     "            0x00..=0x1f if meta => {\n"
     "                out.push(Input::Action(Action::Ignore));\n")]),
    ("M24 a leading zero in a cursor key's modifier is refused", [(I, DIGITS,
     "    if digits.is_empty() || digits.len() > 3 || digits.first() == Some(&b'0') || !digits.iter().all(u8::is_ascii_digit) {\n")]),
    ("M25 a question's choice moves on a prefixed Down (ParagraphDown)", [(Q, Q_DOWN,
     "            Action::Down | Action::ParagraphDown => Some(Self::Down),\n")]),
    ("M26 the pair's 30 ms stage deleted: both Escapes at 50 ms (round 2's flush)", [(I, PAIR, ""),
     (I, LONE, "        if quiet >= Self::ESC_TIMEOUT {\n"
               "            self.stage = Stage::Ground;\n"
               "            out.push(Input::Action(Action::Escape));\n"
               "            if meta {\n"
               "                out.push(Input::Action(Action::Escape));\n"
               "            }\n"
               "        }\n")]),
    ("M27 the pair's stage set to 50 ms", [(I, PAIR_CONST,
     "    pub(crate) const PAIR_TIMEOUT: Duration = Duration::from_millis(50);\n")]),
    ("M28 the pair's stage emits two Escapes at 30 ms and nothing pending", [(I, PAIR,
     "        if meta && quiet >= Self::PAIR_TIMEOUT {\n"
     "            out.push(Input::Action(Action::Escape));\n"
     "            out.push(Input::Action(Action::Escape));\n"
     "            self.stage = Stage::Ground;\n"
     "            return;\n"
     "        }\n")]),
    ("M29 the pair's stage drops the kept ESC (one Escape at 30 ms, nothing pending)", [(I, PAIR,
     "        if meta && quiet >= Self::PAIR_TIMEOUT {\n"
     "            out.push(Input::Action(Action::Escape));\n"
     "            self.stage = Stage::Ground;\n"
     "            return;\n"
     "        }\n")]),
]
only = sys.argv[3:]
if only:
    MUTANTS = [m for m in MUTANTS if m[0].split()[0] in only]
TESTS = [["cargo", "test", "--locked", "--lib", "--", "tui::"]]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, edits in MUTANTS:
    saved, applied = {}, True
    for path, old, new in edits:
        full = os.path.join(repo, path)
        src = saved.setdefault(full, open(full).read())
        now = open(full).read()
        if now.count(old) != 1:
            say(f"{name}: NOT APPLIED ({path}: pattern found {now.count(old)} times)"); applied = False; break
        open(full, "w").write(now.replace(old, new))
    if not applied:
        for full, src in saved.items():
            open(full, "w").write(src)
        continue
    verdict, why = "SURVIVED", ""
    t0 = time.time()
    try:
        for cmd in TESTS:
            try:
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=1200)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", ", ".join(
                    f.removeprefix("tui::input::tests::").removeprefix("tui::shell::tests::")
                    .removeprefix("tui::question::tests::").removeprefix("tui::editor::tests::")
                    for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        for full, src in saved.items():
            open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
