"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the files. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-CTRL-J's mutants.py, with this unit's mutants: one per key the unit
changes at a rule confirmation (M1-M7), Escape's stop over a running turn
and only over one (M8, M9), the activity row under the confirmation (M10)
and the stop's silence (M11)."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
S = "src/tui/shell.rs"
R = "src/tui/rule_confirm.rs"
ENTRY = ("        let Some(action) = Self::approval_action(event, false) else {\n"
         "            return;\n        };\n        let Some(confirm) = self.rule_confirm.as_mut() else {\n")
def before(guard):
    """`guard` (lines of the function's own) ahead of the translation."""
    return (S, ENTRY, guard + ENTRY)
MOVE = "            Action::Up | Action::Down | Action::Tab => {\n"
CANCEL = "            Action::Submit | Action::Cancel => Reply::Cancel,\n"
STOP = ("                if self.activity.working() || self.compacting() {\n"
        "                    if !self.stop_requested || self.work_typed_since_the_stop() {\n")
MODAL = "        self.asking() || self.ask.is_some() || self.rule_confirm.is_some()\n"
SILENT = "                        self.cancel_turn_with(true);\n"
ROUTE = ("            if self.rule_confirm.is_some() {\n"
         "                self.offer_to_rule_confirm(spelled, now);\n")
MUTANTS = [
    ("M1 C-j swallowed at a rule confirmation", [
        before("        if matches!(event, Input::Action(Action::ControlJ)) {\n            return;\n        }\n"),
    ]),
    ("M2 Ctrl-C swallowed at a rule confirmation", [
        (R, CANCEL, "            Action::Submit => Reply::Cancel,\n            Action::Cancel => Reply::Ignored,\n"),
    ]),
    ("M3 Tab moves nothing", [
        (R, MOVE, "            Action::Tab => Reply::Ignored,\n            Action::Up | Action::Down => {\n"),
    ]),
    ("M4 Left, Right, Alt-b and Alt-f move nothing", [
        before("        if matches!(event, Input::Action(Action::Left | Action::Right | Action::AltB | Action::AltF)) {\n"
               "            return;\n        }\n"),
    ]),
    ("M5 C-p and C-n move the mark again", [
        before("        let event = match event {\n"
               "            Input::Action(Action::ControlP) => Input::Action(Action::Up),\n"
               "            Input::Action(Action::HistoryNext) => Input::Action(Action::Down),\n"
               "            other => other,\n        };\n"),
    ]),
    ("M6 the folded event again (Shift-, Ctrl-, Alt-, Super-Up and -Down move)", [
        (S, ROUTE, ROUTE.replace("offer_to_rule_confirm(spelled, now)", "offer_to_rule_confirm(folded(spelled), now)")),
    ]),
    ("M7 Ctrl-L repaints again", [
        before("        if matches!(event, Input::Action(Action::Redraw)) {\n"
               "            self.external_damage();\n            return;\n        }\n"),
    ]),
    ("M8 Escape stops nothing over a running turn", [
        (S, STOP, STOP.replace("self.activity.working() || self.compacting()", "false")),
    ]),
    ("M9 Escape stops a turn whether or not one runs", [
        (S, STOP, STOP.replace("self.activity.working() || self.compacting()", "true")),
    ]),
    ("M10 an activity row under a rule confirmation again", [
        (S, MODAL, "        self.asking() || self.ask.is_some()\n"),
    ]),
    ("M11 Escape's stop writes the turn's cancellation row", [
        (S, SILENT, SILENT.replace("true", "false")),
    ]),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "rule_confirm", "remember", "revoke",
     "escape_closes_the_confirmation", "a_rule_change_waiting"],
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
