"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt> [M<n>...]
Applies each mutant to the clone alone, runs the TUI's unit tests there
(`cargo test --lib -- tui::`) under a per-mutant timeout, and restores the
files. A mutant is killed when a test fails. Never run against the worktree:
the clone must sit under `$TMPDIR/mut/`, and its build and temp files go
beside it there. COMPOSER-PARAGRAPH-MOVE's runner, with this unit's
mutants: the base's Left and Right put back two ways (M1, M2); Home and
End's arms broken one rule at a time -- Alt's missing null, Ctrl's draft,
Super's draft, Shift's remembered selection (M3-M6); the tilde rule read
through the cursor table, without Ctrl's draft, or with the lock bits
dropped (M7-M9); each reader's half undone -- the fold, the question, the
editor (M10-M12). Mutant names after <out.txt> run those alone."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
E = "src/tui/editor.rs"
Q = "src/tui/question.rs"
SUPER_LR = ("        b'D' if held_super => Action::LineStart { shifted },\n"
            "        b'C' if held_super => Action::LineEnd { shifted },\n")
WORD_L = "        b'D' if word => Action::WordLeft,\n"
WORD_R = "        b'C' if word => Action::WordRight,\n"
STEP_L = "        b'D' if shifted => Action::ComposerLeft,\n"
STEP_R = "        b'C' if shifted => Action::ComposerRight,\n"
SUPER_HOME = "        b'A' | b'H' if held_super => Action::DraftStart { shifted },\n"
SUPER_END = "        b'B' | b'F' if held_super => Action::DraftEnd { shifted },\n"
ALT_HOME = "        b'H' if alt => Action::Home,\n"
ALT_END = "        _ if alt => Action::End,\n"
CTRL_HOME = "        b'H' if ctrl => Action::DraftStart { shifted },\n"
CTRL_END = "        _ if ctrl => Action::DraftEnd { shifted },\n"
SHIFT_HOME = "        b'H' if shifted => Action::LineStart { shifted },\n"
TILDE_HOME = ("        1 | 7 if !held => Action::Home,\n"
              "        1 | 7 if draft => Action::DraftStart { shifted },\n"
              "        1 | 7 => Action::LineStart { shifted },\n")
TILDE_END = ("        4 | 8 if !held => Action::End,\n"
             "        4 | 8 if draft => Action::DraftEnd { shifted },\n"
             "        4 | 8 => Action::LineEnd { shifted },\n")
HELD = "    let held = modifiers != 0;\n"
FOLD = ("        Input::Action(Action::LineStart { .. }) => Input::Action(Action::Home),\n"
        "        Input::Action(Action::LineEnd { .. }) => Input::Action(Action::End),\n")
QUESTION = ("            Action::DraftStart { .. } | Action::LineStart { .. } => Some(Self::Home),\n"
            "            Action::DraftEnd { .. } | Action::LineEnd { .. } => Some(Self::End),\n")
EDITOR = "            Action::Home | Action::LineStart { .. } => {\n"
MUTANTS = [
    ("M1 Super-Left and -Right are the step (the base's ComposerLeft)", [(I, SUPER_LR,
     "        b'D' if held_super => Action::ComposerLeft,\n        b'C' if held_super => Action::ComposerRight,\n")]),
    ("M2 Ctrl and Alt are read before Super on Left and Right (the base's order)", [(I, SUPER_LR, ""),
     (I, STEP_L, STEP_L + "        b'D' if held_super => Action::LineStart { shifted },\n"),
     (I, STEP_R, STEP_R + "        b'C' if held_super => Action::LineEnd { shifted },\n")]),
    ("M3 Alt's arm on Home and End falls through to Ctrl's (no null)", [(I, ALT_HOME, ""), (I, ALT_END, "")]),
    ("M4 Ctrl-Home and -End are the line's edges", [(I, CTRL_HOME, "        b'H' if ctrl => Action::LineStart { shifted },\n"),
     (I, CTRL_END, "        _ if ctrl => Action::LineEnd { shifted },\n")]),
    ("M5 Super-Home and -End are the line's edges (Super only on the arrows)", [(I, SUPER_HOME,
     "        b'A' if held_super => Action::DraftStart { shifted },\n        b'H' if held_super => Action::LineStart { shifted },\n"),
     (I, SUPER_END, "        b'B' if held_super => Action::DraftEnd { shifted },\n        b'F' if held_super => Action::LineEnd { shifted },\n")]),
    ("M6 Shift-Home is the bare Home (its selection forgotten in the decoder)", [(I, SHIFT_HOME, "        b'H' if shifted => Action::Home,\n")]),
    ("M7 the tilde Home and End go through the cursor table (Super the draft's edge, Alt the bare key)", [(I, TILDE_HOME,
     "        1 | 7 if !held => Action::Home,\n        1 | 7 => cursor(b'H', modifiers.to_le_bytes()[0]),\n"),
     (I, TILDE_END, "        4 | 8 if !held => Action::End,\n        4 | 8 => cursor(b'F', modifiers.to_le_bytes()[0]),\n")]),
    ("M8 the tilde Home and End are the line's edges under Ctrl too", [(I, TILDE_HOME, TILDE_HOME.replace("if draft", "if draft && false")),
     (I, TILDE_END, TILDE_END.replace("if draft", "if draft && false"))]),
    ("M9 the tilde keys read a lock bit alone as no modifier", [(I, HELD, "    let held = modifiers & 0x3f != 0;\n")]),
    ("M10 the shell does not fold the line's edges for the menus and the composer", [(S, FOLD, "")]),
    ("M11 a question reads the line's edges as nothing", [(Q, QUESTION,
     "            Action::DraftStart { .. } => Some(Self::Home),\n            Action::DraftEnd { .. } => Some(Self::End),\n")]),
    ("M12 the editor's line start is the line's end", [(E, EDITOR,
     "            Action::LineStart { .. } => {\n                self.move_to(self.line_end());\n                None\n            }\n            Action::Home => {\n")]),
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
