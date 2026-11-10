"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the files. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-CTRL-J's mutants.py, with this unit's mutants: M1-M3 the brief's
three (M3 split by spelling), M4-M6 where and how C-j answers, M7 the fix
taken out, M8-M9 /mcp's held-Enter rule."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
S = "src/tui/shell.rs"
I = "src/tui/input.rs"
Q = "src/tui/question.rs"
ANSWER = "            Input::Action(action) => match question::Act::of(action) {\n"
AT_A_CHOICE = ("            Input::Action(Action::ControlJ)\n"
               "                if self.ask.as_ref().is_some_and(|ask| {\n"
               "                    ask.caret(self.geometry.cols, self.geometry.rows).is_none()\n"
               "                }) =>\n            {\n                return\n            }\n")
IN_THE_FIELD = AT_A_CHOICE.replace(".is_none()", ".is_some()")
INSERTS = ("            Input::Action(Action::ControlJ)\n"
           "                if self.ask.as_ref().is_some_and(|ask| {\n"
           "                    ask.caret(self.geometry.cols, self.geometry.rows).is_some()\n"
           "                }) =>\n            {\n                question::Act::Text('\\n')\n            }\n")
ARM = "            Action::Submit | Action::ControlJ => Some(Self::Submit),\n"
KITTY = '        None | Some(b"1" | b"2") => kitty_key(code, mask),\n'
XTERM = "                (Some(mask), Some(code)) => kitty_key(code, mask),\n"
CTRL_LETTER = "    if mods & CTRL != 0 {\n        if let Some(byte @ (b'a'..=b'z' | b'A'..=b'Z' | b'_')) = ascii {\n"
FOCUS = "            if self.ask.is_some() {\n                self.answer(event, now);\n"
COUNTS = "                Input::Action(Action::ControlJ) => self.ask.is_some(),\n"
MUTANTS = [
    # The brief's three.
    ("M1 C-j dropped at a choice", [(S, ANSWER, AT_A_CHOICE + ANSWER)]),
    ("M2 C-j dropped in the field", [(S, ANSWER, IN_THE_FIELD + ANSWER)]),
    ("M3a kitty's CSI u Ctrl+J dropped", [
        (I, KITTY, '        None | Some(b"1" | b"2") if code == 106 || code == 74 => ignore,\n' + KITTY),
    ]),
    ("M3b xterm's CSI 27;m;k~ Ctrl+J dropped", [
        (I, XTERM, "                (Some(_), Some(106 | 74)) => ignore,\n" + XTERM),
    ]),
    ("M3c a Ctrl+J with another modifier held not replayed (Ctrl alone only)", [
        (I, CTRL_LETTER, CTRL_LETTER.replace("mods & CTRL != 0", "mods == CTRL")),
    ]),
    ("M3d a kitty press or repeat (an event-type field) dropped", [
        (I, KITTY, "        None => kitty_key(code, mask),\n"),
    ]),
    # Where and how C-j answers.
    ("M4 C-j moves the choice down, as a menu's C-j", [
        (Q, ARM, "            Action::Submit => Some(Self::Submit),\n"
                 "            Action::ControlJ => Some(Self::Down),\n"),
    ]),
    ("M5 C-j in the field is a newline, as in the composer", [(S, ANSWER, INSERTS + ANSWER)]),
    ("M6 C-j goes to the /status panel under the question", [
        (S, FOCUS, "            if matches!(event, Input::Action(Action::ControlJ))\n"
                   "                && self.info_panel.is_some()\n"
                   "                && self.offer_to_info_panel(&event)\n"
                   "            {\n                continue;\n            }\n" + FOCUS),
    ]),
    # The fix taken out whole: the base.
    ("M7 C-j swallowed at a model question again (the base)", [
        (Q, ARM, "            Action::Submit => Some(Self::Submit),\n"),
    ]),
    # /mcp's held-Enter rule.
    ("M8 a C-j the model's question takes not counted for the held rule", [
        (S, COUNTS, "                Input::Action(Action::ControlJ) => false,\n"),
    ]),
    ("M9 a C-j the menu takes counted for the held rule too", [
        (S, COUNTS, "                Input::Action(Action::ControlJ) => true,\n"),
    ]),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "model_question", "question::", "input::tests", "held_rule"],
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, edits in MUTANTS:
    saved = {}
    applied = True
    for path, old, new in edits:
        full = os.path.join(repo, path)
        if full not in saved:
            saved[full] = open(full).read()
        src = open(full).read()
        if src.count(old) != 1:
            say(f"{name}: NOT APPLIED ({path}: pattern found {src.count(old)} times)")
            applied = False
            break
        open(full, "w").write(src.replace(old, new))
    if not applied:
        for full, src in saved.items():
            open(full, "w").write(src)
        continue
    verdict, why = "SURVIVED", ""
    t0 = time.time()
    try:
        for cmd in TESTS:
            try:
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=900)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", f"{len(failed)} failed: " + ", ".join(failed)
                break
    finally:
        for full, src in saved.items():
            open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
