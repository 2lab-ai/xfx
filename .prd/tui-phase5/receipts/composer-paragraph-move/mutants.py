"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the TUI's unit tests there
(`cargo test --lib -- tui::`) under a per-mutant timeout, and restores the
files. A mutant is killed when a test fails. Never run against the worktree:
the clone must sit under `$TMPDIR/mut/`, and its build and temp files go
beside it there. MODIFIED-ARROWS-SCREENS' runner, with this unit's mutants
and more than one edit to a mutant: the base's fold and decoder put back
(M1, M2); each held key's composer move made the arrow's or another's
(M3-M7); the paragraph boundaries' three rules broken one at a time
(M8-M10); the `/model` column kept (M11); the pickers' fold wrong three ways
(M12-M14); a compact menu reading the pickers' fold (M15); the full
transcript and the question reading Alt-Up as Up (M16, M17)."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
E = "src/tui/editor.rs"
F = "src/tui/full_transcript.rs"
Q = "src/tui/question.rs"
ALT_F = "        Input::Action(Action::AltF) => Input::Action(Action::WordRight),\n"
BASE_FOLD = (ALT_F +
             "        Input::Action(Action::ComposerUp | Action::SelectUp | Action::ParagraphUp | Action::DraftStart { .. }) => Input::Action(Action::Up),\n"
             "        Input::Action(Action::ComposerDown | Action::SelectDown | Action::ParagraphDown | Action::DraftEnd { .. }) => Input::Action(Action::Down),\n")
ALT_UP = "        b'A' if alt => Action::ParagraphUp,\n"
CTRL = "            Action::ComposerUp => self.act(Action::Up, now),\n"
ROW = ("                if self.editor.move_rows(step, 1, self.text_cols()) {\n"
       "                    self.moved();\n"
       "                } else {\n"
       "                    self.editor.forget_column();\n"
       "                }\n")
FORGET = "                    self.editor.forget_column();\n"
CARET = ("            | Action::ParagraphUp\n"
         "            | Action::ParagraphDown\n"
         "            | Action::DraftStart { .. }\n"
         "            | Action::DraftEnd { .. } => {\n"
         "                let recorded")
BLANK = "matches!(byte, b' ' | b'\\t' | b'\\r')"
CROSS = "    if cursor > paragraph_start {\n"
SKIP = "    for blank in [false, true] {\n"
COLUMN = ("                | Action::WordRight\n"
          "                | Action::ParagraphUp\n"
          "                | Action::ParagraphDown\n"
          "                | Action::DraftStart { .. }\n"
          "                | Action::DraftEnd { .. },\n"
          "            ) => {\n")
PICKERS = "        Input::Action(Action::ComposerUp | Action::SelectUp) => Input::Action(Action::Up),\n"
MENUS = "            let menus = arrow_to_pickers(&event);\n"
FOLD = "            let event = folded(event);\n"
SCREEN = "            Action::Up => moved(true, Some(ARROW_ROWS)),\n"
QUESTION = "            Action::Up => Some(Self::Up),\n"
MUTANTS = [
    ("M1 the shell folds every held Up and Down into the arrow (the base's fold)", [(S, ALT_F, BASE_FOLD)]),
    ("M2 the decoder reads Alt-Up as Ctrl's (the base's ComposerUp)", [(I, ALT_UP, ALT_UP.replace("ParagraphUp", "ComposerUp"))]),
    ("M3 Shift-Up is the plain arrow, edge and history included", [(S, ROW,
     "                let _ = step;\n                self.act(if action == Action::SelectUp { Action::Up } else { Action::Down }, now);\n")]),
    ("M4 Shift-Up with no row above keeps the run's column", [(S, FORGET, "")]),
    ("M5 Alt-Up and -Down are the plain arrows", [(S, CARET, CARET.replace("            | Action::ParagraphUp\n            | Action::ParagraphDown\n", "")),
     (S, CTRL, "            Action::ParagraphUp => self.act(Action::Up, now),\n            Action::ParagraphDown => self.act(Action::Down, now),\n" + CTRL)]),
    ("M6 Super-Up and -Down are the plain arrows", [(S, CARET, CARET.replace("            | Action::DraftStart { .. }\n            | Action::DraftEnd { .. } => {", "            => {")),
     (S, CTRL, "            Action::DraftStart { .. } => self.act(Action::Up, now),\n            Action::DraftEnd { .. } => self.act(Action::Down, now),\n" + CTRL)]),
    ("M7 Ctrl-Up stops at the draft's first row, as Shift's does", [(S, CTRL, CTRL.replace("Action::Up, now", "Action::SelectUp, now"))]),
    ("M8 a line of spaces is no blank line", [(E, BLANK, "matches!(byte, b'\\t' | b'\\r')")]),
    ("M9 Alt-Up never crosses into the paragraph before", [(E, CROSS, CROSS.replace(">", ">="))]),
    ("M10 Alt-Down stops at the blank line after the paragraph", [(E, SKIP, SKIP.replace("[false, true]", "[false]"))]),
    ("M11 Alt and Super keep a later /model column", [(S, COLUMN,
     "                | Action::WordRight,\n            ) => {\n")]),
    ("M12 the pickers do not walk on Shift", [(S, PICKERS, PICKERS.replace(" | Action::SelectUp", ""))]),
    ("M13 the pickers walk on Alt", [(S, PICKERS, PICKERS.replace("Action::SelectUp)", "Action::SelectUp | Action::ParagraphUp)"))]),
    ("M14 the pickers read the held keys unfolded", [(S, MENUS, "            let menus = event.clone();\n")]),
    ("M15 the compact menus and a rule confirmation read the pickers' fold", [(S, FOLD, "            let event = arrow_to_pickers(&folded(event));\n")]),
    ("M16 the full transcript scrolls on Alt-Up", [(F, SCREEN, SCREEN.replace("Action::Up =>", "Action::Up | Action::ParagraphUp =>"))]),
    ("M17 a question moves its choice on Alt-Up", [(Q, QUESTION, QUESTION.replace("Action::Up =>", "Action::Up | Action::ParagraphUp =>"))]),
]
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
