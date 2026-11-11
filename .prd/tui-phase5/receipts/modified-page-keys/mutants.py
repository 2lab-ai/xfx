"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
TILDE-EMPTY-MODIFIER's runner, with this unit's mutants: the decoder's two
readings put back one spelling at a time (M1-M5), the fold's three ways to be
wrong (M6-M8), and the full transcript taking the held key (M9). Round 2 adds
the modal drafts taking the held key as a caret move (M10 the question's
field, M11 the approval's amendment) and a page behind the screen one row
short (M12), each for the test round 2 added."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
F = "src/tui/full_transcript.rs"
HELD = "    let held = modifiers != 0;\n"
TILDE_PAGES = "        5 if held => Action::ComposerPageUp,\n        6 if held => Action::ComposerPageDown,\n"
KEYPAD = "            return Input::Action(match (up, mods != 0) {\n"
FOLD = ("        Input::Action(Action::ComposerPageUp) => Input::Action(Action::PageUp),\n"
        "        Input::Action(Action::ComposerPageDown) => Input::Action(Action::PageDown),\n")
UNSPELLED = "        Input::Action(Action::AltF) => Input::Action(Action::WordRight),\n"
SCREEN = "            Action::PageUp => moved(true, None),\n"
Q = "src/tui/question.rs"
QUESTION_UP = "            Action::Up => Some(Self::Up),\n"
DRAFT_EDGE = "            Input::Action(edge @ (Action::DraftStart { .. } | Action::DraftEnd { .. }))\n"
PAGE = "                let page = usize::from(layout::input_row_limit(self.geometry.rows));\n"
MUTANTS = [
    ("M1 a held tilde page key is the page key (the base's reading)", I, TILDE_PAGES, ""),
    ("M2 a held keypad page key is the page key (the base's reading)", I, KEYPAD,
     "            return Input::Action(match (up, false) {\n"),
    ("M3 the keypad counts a lock bit alone as held", I, KEYPAD,
     "            return Input::Action(match (up, modifiers != 0) {\n"),
    ("M4 the tilde page key reads only the field's low byte", I, HELD,
     "    let held = low != 0;\n"),
    ("M5 the tilde page key masks the lock bits off, as the keypad does", I, HELD,
     "    let held = modifiers & 0x3f != 0;\n"),
    ("M6 no fold: the held key reaches the lists and the composer as itself", S, FOLD, ""),
    ("M7 the fold makes a held page key the arrow", S, FOLD,
     FOLD.replace("Action::PageUp),", "Action::Up),").replace("Action::PageDown),", "Action::Down),")),
    ("M8 the fold runs before the full transcript reads the event", S, UNSPELLED,
     UNSPELLED + FOLD),
    ("M9 the full transcript pages on the held key too", F, SCREEN,
     "            Action::PageUp | Action::ComposerPageUp => moved(true, None),\n"),
    # Round 2.
    ("M10 a question's freeform field takes a held page key as Home", Q, QUESTION_UP,
     "            Action::ComposerPageUp | Action::ComposerPageDown => Some(Self::Home),\n" + QUESTION_UP),
    ("M11 an approval's amendment takes a held page key as Home", S, DRAFT_EDGE,
     "            Input::Action(Action::ComposerPageUp | Action::ComposerPageDown) if drafting => {\n"
     "                approval::Action::Edit(Action::Home)\n"
     "            }\n" + DRAFT_EDGE),
    ("M12 a page behind the screen is one row short of the composer's cap", S, PAGE,
     PAGE.replace("self.geometry.rows));", "self.geometry.rows)).saturating_sub(1);")),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "tui::input::tests",
     "a_page_key_held_with_a_modifier", "the_fold_maps", "at_41_rows",
     "the_screens_keys_move_it"],
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, path, old, new in MUTANTS:
    full = os.path.join(repo, path)
    src = open(full).read()
    if src.count(old) != 1:
        say(f"{name}: NOT APPLIED (pattern found {src.count(old)} times)"); continue
    open(full, "w").write(src.replace(old, new))
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
                verdict, why = "KILLED", ", ".join(
                    f.removeprefix("tui::input::tests::").removeprefix("tui::shell::tests::")
                    for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
