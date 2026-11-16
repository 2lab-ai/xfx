"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the files. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-DRAFT-KEYS' mutants.py, with this unit's mutants (M1-M3 the brief's,
M3 split by spelling; M4-M7 where C-j answers; M8 the fix taken out) and a
mutant made of one or more edits."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
S = "src/tui/shell.rs"
I = "src/tui/input.rs"
ARM = "            Input::Action(Action::Submit | Action::ControlJ) => approval::Action::Submit,\n"
DRAFTING = "        let drafting = self.drafting();\n        let action = match event {\n"
NOTED = ("        let drafting = self.drafting();\n"
         "        let ctrl_j = matches!(event, Input::Action(Action::ControlJ));\n"
         "        let action = match event {\n")
GATE = "        if matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always)\n"
FOCUS = "            if self.asking() {\n                self.decide(spelled, now);\n"
KITTY = '        None | Some(b"1" | b"2") => kitty_key(code, mask),\n'
XTERM = "                (Some(mask), Some(code)) => kitty_key(code, mask),\n"
CTRL_LETTER = "    if mods & CTRL != 0 {\n        if let Some(byte @ (b'a'..=b'z' | b'A'..=b'Z' | b'_')) = ascii {\n"
MUTANTS = [
    # The brief's three.
    ("M1 C-j not gated by readiness", [
        (S, DRAFTING, NOTED),
        (S, GATE, "        if matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always) && !ctrl_j\n"),
    ]),
    ("M2 C-j at Deny gated", [
        (S, DRAFTING, NOTED),
        (S, GATE, "        if (matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always) || ctrl_j)\n"),
    ]),
    ("M3a kitty's CSI u Ctrl+J dropped", [
        (I, KITTY, '        None | Some(b"1" | b"2") if code == 106 || code == 74 => ignore,\n' + KITTY),
    ]),
    ("M3b xterm's CSI 27;m;k~ Ctrl+J dropped", [
        (I, XTERM, "                (Some(_), Some(106 | 74)) => ignore,\n" + XTERM),
    ]),
    ("M3c a Ctrl+J with another modifier held not replayed (Ctrl alone only)", [
        (I, CTRL_LETTER, CTRL_LETTER.replace("mods & CTRL != 0", "mods == CTRL")),
    ]),
    # Where C-j answers.
    ("M4 C-j answers at a choice only, not in a draft", [
        (S, ARM, "            Input::Action(Action::Submit) => approval::Action::Submit,\n"
                 "            Input::Action(Action::ControlJ) if !drafting => approval::Action::Submit,\n"),
    ]),
    ("M5 C-j moves the choice down, as a menu's C-j", [
        (S, ARM, "            Input::Action(Action::Submit) => approval::Action::Submit,\n"
                 "            Input::Action(Action::ControlJ) => approval::Action::Down,\n"),
    ]),
    ("M6 C-j swallowed on the review plane", [
        (S, ARM, "            Input::Action(Action::ControlJ) if self.alternate.is_some() => return,\n" + ARM),
    ]),
    ("M7 C-j goes to the /status panel under the question", [
        (S, FOCUS, "            if matches!(event, Input::Action(Action::ControlJ))\n"
                   "                && self.info_panel.is_some()\n"
                   "                && self.offer_to_info_panel(&event)\n"
                   "            {\n                continue;\n            }\n" + FOCUS),
    ]),
    # The fix taken out whole: the base.
    ("M8 C-j swallowed at a question again (the base)", [
        (S, ARM, "            Input::Action(Action::Submit) => approval::Action::Submit,\n"),
    ]),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "approval", "amendment", "alt_b", "alt_f", "the_fold",
     "horizontal", "draft_left", "up_and_down_move", "the_arrows_leave", "leaving_a_filled_draft",
     "revokes", "vertical", "super_up_and_down", "one_read", "until_a_frame", "refusal_read",
     "a_walk_of_the_review", "enter_takes_the_marked", "writes_nothing", "ctrl_l",
     "undo_redo_and_yank", "tab_in_a_draft", "every_key_fxs", "the_word_deletes", "an_edit_then_a_key",
     "ctrl_j_", "keys_fx_does_not_read_as_ctrl_j"],
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
