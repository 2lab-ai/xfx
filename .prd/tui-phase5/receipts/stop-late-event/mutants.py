"""STOP-LATE-EVENT's code mutants, run on a --no-local clone (never the worktree).

usage: python3 -I -B mutants.py <clone> <target-dir> <out-file> [n,n,...]

The optional last argument runs only those mutants (numbered from 1, in
MUTANTS' order); the out file numbers each row.

Each mutant is one exact replacement (or two) in src/tui/shell.rs or
src/tui/worker.rs. The clone
is restored with `git checkout -- .` after each. A mutant is killed when its
command fails with a test that failed; one that fails to build is
NOCOMPILE, which counts as no kill (the mutant is wrong, not caught). TEST runs the whole TUI library suite (`tui::`), so a mutant
that survives survives every TUI test, not only this unit's.
"""
import os
import subprocess
import sys

clone, target, out = sys.argv[1:4]
only = {int(n) for n in sys.argv[4].split(",")} if len(sys.argv) > 4 else None
# TMPDIR is the caller's: set it short (the round-2 runs used a short scratch directory).
env = dict(os.environ, CARGO_TARGET_DIR=target)
for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(name, None)

TEST = ["cargo", "test", "--locked", "--lib", "--", "tui::"]
SHELL = "src/tui/shell.rs"
WORKER = "src/tui/worker.rs"

GUARD = "        if self.cancelling && self.work.outstanding() == 0 && self.tally.started.is_none() {\n"
SETTLE = (
    GUARD
    + "            self.cancelling = false;\n"
    + "            self.stop_reads_end = false;\n"
    + "            self.cancel_owed = None;\n"
    + "            self.cancel_covers = 0;\n"
    + "        }\n"
)
POP = "                let (input, title, card, _) = self.prompts.pop_front().unwrap_or_default();\n"
DRAINED_END = (
    "            UiEvent::TurnEnded { .. } => {\n"
    "                self.tally = Tally::default();\n"
)
BEHIND = (
    "        let behind =\n"
    "            self.tally.started.is_some() || self.pacer.pending() > 0 || self.full.is_open();\n"
)
HOLD = "        if working && !self.activity.working() && !compacting && !behind_a_turn {\n"
GATE = "            self.work.outstanding() > 0 && self.prompts.front().is_some_and(|prompt| prompt.3);\n"
QUIET = (
    "                let quiet = stopped\n"
    "                    && (!reads_end || failure.as_deref() == Some(super::bridge::TURN_CANCELLED));\n"
)
STEER = "                        Some((card, BadgeLinks::new(), SkillRuns::new())),\n                        true,\n"

# Each mutant: a title and its edits, (path, original, replacement); the
# original must occur exactly once.
MUTANTS = [
    # Item 1: the stop window.
    ("item 1: the settle takes a started turn's cancel (the base)",
     [(SHELL, GUARD, "        if self.cancelling && self.work.outstanding() == 0 {\n")]),
    ("item 1: the guard inverted (only a started turn's cancel is taken)",
     [(SHELL, GUARD, GUARD.replace("is_none()", "is_some()"))]),
    ("item 1: the settle's cancel clear deleted (what the settle is for)",
     [(SHELL, SETTLE, "")]),
    # Item 2: a prompt fx would queue at once.
    ("item 2: the hold ignores the marker (the base: held behind a Thinking frame)",
     [(SHELL, HOLD, "        if working && !self.activity.working() && !compacting {\n")]),
    ("item 2: no prompt is marked",
     [(SHELL, BEHIND, "        let behind = false;\n")]),
    ("item 2: every prompt is marked (an idle session's prompt reported ahead of its Thinking)",
     [(SHELL, BEHIND, "        let behind = true;\n")]),
    ("item 2: marked while the turn's activity runs, not while it is in hand",
     [(SHELL, BEHIND, "        let behind = waits || self.pacer.pending() > 0 || self.full.is_open();\n")]),
    ("item 2: the pacer not read (round 1's marker)",
     [(SHELL, BEHIND, "        let behind = self.tally.started.is_some() || self.full.is_open();\n")]),
    ("item 2: the full transcript not read",
     [(SHELL, BEHIND, "        let behind = self.tally.started.is_some() || self.pacer.pending() > 0;\n")]),
    ("item 2: a steer handed on after its turn is not marked (round 1's 8)",
     [(SHELL, STEER, STEER.replace("true,", "false,"))]),
    ("item 2: a dropped prompt's entry still marks work (round 1's gate)",
     [(SHELL, GATE, "            self.prompts.front().is_some_and(|prompt| prompt.3);\n")]),
    ("item 2: round 1's shape -- the turn's start holds its report (frame first)",
     [(SHELL, POP,
       "                if !self.cancelling && !self.prompts.is_empty() {\n"
       "                    self.status_held = true;\n"
       "                }\n" + POP)]),
    # Item 3: `/new` and `compaction_failed`.
    ("item 3: the reset keeps a failed compaction's mark (the base)",
     [(SHELL, "        self.compaction_failed = false;\n        self.retry = None;\n",
       "        self.retry = None;\n")]),
    ("item 3: the mark dropped by the drained end instead of the reset",
     [(SHELL, "        self.compaction_failed = false;\n        self.retry = None;\n",
       "        self.retry = None;\n"),
      (SHELL, DRAINED_END, DRAINED_END + "                self.compaction_failed = false;\n")]),
    # Round 2: a stopped turn's end.
    ("end: every end of a stopped turn is quiet (round 1)",
     [(SHELL, QUIET, "                let quiet = stopped;\n")]),
    ("end: the user's stop not marked as reading the end",
     [(SHELL, "        self.stop_reads_end = true;\n", "")]),
    ("end: a compaction's ending leaves the stop reading the end",
     [(SHELL, "                    self.cancelling = true;\n                    self.stop_reads_end = false;\n",
       "                    self.cancelling = true;\n")]),
    ("end: a real failure after the stop read as the stop's own",
     [(SHELL, QUIET, "                let quiet = stopped && (!reads_end || failure.is_some());\n")]),
    ("end: a stopped turn titles nothing (the base)",
     [(SHELL, "                        if quiet && ran && !noted {\n", "                        if false {\n")]),
    ("end: a turn an automatic compaction failed titles the session",
     [(SHELL, "                        if quiet && ran && !noted {\n", "                        if quiet && ran {\n")]),
    ("runtime: a refused send's stop not said in the stop's word",
     [(WORKER, "        Err(crate::agent::TurnError::Sink(err)) if err.kind() == io::ErrorKind::Interrupted => {\n",
       "        Err(crate::agent::TurnError::Sink(err)) if false && err.kind() == io::ErrorKind::Interrupted => {\n")]),
    ("runtime: the log's failure on a stopped turn dropped",
     [(WORKER, "        Err(crate::agent::TurnError::Cancelled) => {\n            Some(log_failure.unwrap_or_else(|| bridge::TURN_CANCELLED.to_string()))\n",
       "        Err(crate::agent::TurnError::Cancelled) => {\n            Some(bridge::TURN_CANCELLED.to_string())\n")]),
    ("runtime: a compaction that compacted nothing ends the turn (the base)",
     [(WORKER, "        compaction.is_some_and(|feedback| !matches!(feedback, Feedback::NoOp | Feedback::Busy));\n",
       "        compaction.is_some();\n")]),
]

lines = []
for number, (title, edits) in enumerate(MUTANTS, 1):
    if only is not None and number not in only:
        continue
    texts = {}
    for path, _, _ in edits:
        texts.setdefault(path, open(os.path.join(clone, path), encoding="utf-8").read())
    bad = [(path, old) for path, old, _ in edits if texts[path].count(old) != 1]
    if bad:
        lines.append("%2d  SKIP      %-80s %s: the original occurs %d times" % (number, title, bad[0][0], texts[bad[0][0]].count(bad[0][1])))
        print(lines[-1], flush=True)
        continue
    for path, old, new in edits:
        texts[path] = texts[path].replace(old, new)
    for path, text in texts.items():
        open(os.path.join(clone, path), "w", encoding="utf-8").write(text)
    run = subprocess.run(TEST, cwd=clone, env=env, capture_output=True, text=True, timeout=1800)
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, env=env, check=True)
    tail = (run.stdout + run.stderr).strip().splitlines()
    failed = [l for l in tail if l.startswith("test ") and l.endswith("FAILED")]
    err = [l for l in tail if l.startswith("error") and "could not compile" not in l][:1]
    built = not any(l.startswith("error[") or "could not compile" in l for l in tail)
    verdict = "SURVIVED" if run.returncode == 0 else ("KILLED" if failed else ("NOCOMPILE" if not built else "KILLED"))
    detail = "; ".join(f[5:-10] for f in failed[:4]) or (err[0] if err else "exit %d" % run.returncode)
    lines.append("%2d  %-9s %-80s %s" % (number, verdict, title, detail))
    print(lines[-1], flush=True)
open(out, "w").write("\n".join(lines) + "\n")
