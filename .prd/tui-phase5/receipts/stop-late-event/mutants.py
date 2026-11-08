"""STOP-LATE-EVENT's code mutants, run on a --no-local clone (never the worktree).

usage: python3 -I -B mutants.py <clone> <target-dir> <out-file>

Each mutant is one exact replacement (or two) in src/tui/shell.rs. The clone
is restored with `git checkout -- .` after each. A mutant is killed when its
command fails. TEST runs the whole TUI library suite (`tui::`), so a mutant
that survives survives every TUI test, not only this unit's.
"""
import os
import subprocess
import sys

clone, target, out = sys.argv[1:4]
env = dict(os.environ, CARGO_TARGET_DIR=target, TMPDIR="/private/tmp/claude-501/stoplate-m")
for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(name, None)

TEST = ["cargo", "test", "--locked", "--lib", "--", "tui::"]
SHELL = "src/tui/shell.rs"

GUARD = "        if self.cancelling && self.work.outstanding() == 0 && self.tally.started.is_none() {\n"
SETTLE = (
    GUARD
    + "            self.cancelling = false;\n"
    + "            self.cancel_owed = None;\n"
    + "            self.cancel_covers = 0;\n"
    + "        }\n"
)
POP = "                let (input, title, card, _) = self.prompts.pop_front().unwrap_or_default();\n"
DRAINED_END = (
    "            UiEvent::TurnEnded { .. } => {\n"
    "                self.tally = Tally::default();\n"
)

MUTANTS = [
    # Item 1: the stop window.
    ("item 1: the settle takes a started turn's cancel (the base)",
     [(GUARD, "        if self.cancelling && self.work.outstanding() == 0 {\n")]),
    ("item 1: the guard inverted (only a started turn's cancel is taken)",
     [(GUARD, GUARD.replace("is_none()", "is_some()"))]),
    ("item 1: the settle's cancel clear deleted (what the settle is for)",
     [(SETTLE, "")]),
    # Item 2: a prompt sent while a started turn is in hand.
    ("item 2: the hold ignores the marker (the base: held behind a Thinking frame)",
     [("        if working && !self.activity.working() && !compacting && !behind_a_turn {\n",
       "        if working && !self.activity.working() && !compacting {\n")]),
    ("item 2: no prompt is marked",
     [("        let behind = self.tally.started.is_some();\n", "        let behind = false;\n")]),
    ("item 2: every prompt is marked (an idle session's prompt reported ahead of its Thinking)",
     [("        let behind = self.tally.started.is_some();\n", "        let behind = true;\n")]),
    ("item 2: marked while the turn's activity runs, not while it is in hand",
     [("        let behind = self.tally.started.is_some();\n", "        let behind = waits;\n")]),
    ("item 2: a steer handed on after its turn is not marked (expected to survive: see README)",
     [("                        Some((card, BadgeLinks::new(), SkillRuns::new())),\n                        true,\n",
       "                        Some((card, BadgeLinks::new(), SkillRuns::new())),\n                        false,\n")]),
    ("item 2: round 1's shape -- the turn's start holds its report (frame first)",
     [(POP,
       "                if !self.cancelling && !self.prompts.is_empty() {\n"
       "                    self.status_held = true;\n"
       "                }\n" + POP)]),
    # Item 3: `/new` and `compaction_failed`.
    ("item 3: the reset keeps a failed compaction's mark (the base)",
     [("        self.compaction_failed = false;\n        self.retry = None;\n",
       "        self.retry = None;\n")]),
    ("item 3: the mark dropped by the drained end instead of the reset",
     [("        self.compaction_failed = false;\n        self.retry = None;\n",
       "        self.retry = None;\n"),
      (DRAINED_END, DRAINED_END + "                self.compaction_failed = false;\n")]),
]

lines = []
for title, edits in MUTANTS:
    path = os.path.join(clone, SHELL)
    text = open(path, encoding="utf-8").read()
    bad = [old for old, _ in edits if text.count(old) != 1]
    if bad:
        lines.append("SKIP     %-80s the original occurs %d times" % (title, text.count(bad[0])))
        print(lines[-1], flush=True)
        continue
    for old, new in edits:
        text = text.replace(old, new)
    open(path, "w", encoding="utf-8").write(text)
    run = subprocess.run(TEST, cwd=clone, env=env, capture_output=True, text=True, timeout=1800)
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, env=env, check=True)
    tail = (run.stdout + run.stderr).strip().splitlines()
    failed = [l for l in tail if l.startswith("test ") and l.endswith("FAILED")]
    err = [l for l in tail if l.startswith("error") and "could not compile" not in l][:1]
    verdict = "KILLED" if run.returncode != 0 else "SURVIVED"
    detail = "; ".join(f[5:-10] for f in failed[:4]) or (err[0] if err else "exit %d" % run.returncode)
    lines.append("%-8s %-80s %s" % (verdict, title, detail))
    print(lines[-1], flush=True)
open(out, "w").write("\n".join(lines) + "\n")
