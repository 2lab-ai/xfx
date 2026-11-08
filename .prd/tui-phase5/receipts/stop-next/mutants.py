"""STOP-NEXT's code mutants, run on a --no-local clone (never the worktree).

usage: python3 -I -B mutants.py <clone> <target-dir> <out-file> [n,n,...]

STOP-LATE-EVENT's runner (`../stop-late-event/mutants.py`) with this unit's
rows and a per-mutant time limit: a mutant whose suite runs past it is
KILLED (hang). Each mutant is one exact replacement (or two) in
src/tui/shell.rs or src/tui/worker.rs; the original must occur exactly once,
or the row is SKIP. The clone is restored with `git checkout -- .` after
each. A mutant is killed when its command fails with a test that failed;
one that fails to build is NOCOMPILE, which counts as no kill. TEST runs the
whole TUI library suite (`tui::`), so a mutant that survives survives every
TUI test, not only this unit's.
"""
import os
import subprocess
import sys

clone, target, out = sys.argv[1:4]
only = {int(n) for n in sys.argv[4].split(",")} if len(sys.argv) > 4 else None
# TMPDIR is the caller's: set it short.
env = dict(os.environ, CARGO_TARGET_DIR=target)
for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(name, None)

TEST = ["cargo", "test", "--locked", "--lib", "--", "tui::"]
LIMIT = 900
SHELL = "src/tui/shell.rs"
WORKER = "src/tui/worker.rs"

COVERED = "                    .is_some_and(|through| submission <= through)\n"
COVERED_BRANCH = (
    "                {\n"
    "                    self.cancelling = true;\n"
    "                    self.stop_reads_end = true;\n"
    "                    self.sound.interrupted();\n"
    "                } else {\n"
)
UNCOVERED = (
    "                    self.stop_reads_end = false;\n"
    "                    self.stop_requested = false;\n"
    "                }\n"
)
SWEEP = (
    "                while self\n"
    "                    .prompts\n"
    "                    .front()\n"
    "                    .is_some_and(|prompt| prompt.submission < submission)\n"
    "                {\n"
    "                    self.prompts.pop_front();\n"
    "                }\n"
)
TAKE = (
    "                } = if self\n"
    "                    .prompts\n"
    "                    .front()\n"
    "                    .is_some_and(|prompt| prompt.submission == submission)\n"
    "                {\n"
    "                    self.prompts.pop_front().unwrap_or_default()\n"
    "                } else {\n"
    "                    WaitingPrompt::default()\n"
    "                };\n"
)
CARD = "                if let Some((card, links, skills)) = card {\n                    self.echo(&card, links, skills);\n"
WAITING = "            .filter(move |prompt| !through.is_some_and(|through| prompt.submission <= through))\n"
TURN_STOP = (
    "        // or stops its turn if it took it first ([`Self::cancel_through`]).\n"
    "        self.cancel_through = Some(through);\n"
    "        self.stop_requested = true;\n"
)
COMPACTION_STOP = (
    "        self.work.control(TurnControl::Cancel { through });\n"
    "        self.cancel_through = Some(through);\n"
    "        self.stop_requested = true;\n"
    "        let automatic"
)
GUARD = "        if self.stop_requested && !manual && !self.work.switch_pending() {\n            return;\n        }\n"
SETTLE = "        if self.work.outstanding() == 0 {\n            self.stop_requested = false;\n        }\n"
HOLD = "                let (card, echoed) = if waits || behind {\n"
SHOWN = "            .filter(|prompt| prompt.shown)\n"
LEAD = "                        if quiet && ran && !noted && self.title_session() {\n                            self.title_leads = true;\n"
RETITLE = "        self.title_owed = true;\n        self.title_leads = false;\n"
NUMBER = "                let submission = self.work.accepted();\n"
HANDED = "                        submission: self.work.accepted(),\n"
STARTED = "        submission: queue.taken,\n"

# Each mutant: a title and its edits, (path, original, replacement).
MUTANTS = [
    # Items 1-3: which turn a start is.
    ("1-3: a start covered only below the stop's count (off by one)",
     [(SHELL, COVERED, COVERED.replace("<=", "<"))]),
    ("1-3: no start covered (the count ignored)",
     [(SHELL, COVERED, "                    .is_some_and(|_| false)\n")]),
    ("1-3: every start after a stop covered",
     [(SHELL, COVERED, "                    .is_some_and(|_| true)\n")]),
    ("1-3: a covered start does not let its end say what it came to",
     [(SHELL, COVERED_BRANCH, COVERED_BRANCH.replace("                    self.stop_reads_end = true;\n", ""))]),
    ("1-3: a covered start rings the turn-end bell",
     [(SHELL, COVERED_BRANCH, COVERED_BRANCH.replace("                    self.sound.interrupted();\n", ""))]),
    ("1-3: a covered start's card not written (the base)",
     [(SHELL, CARD, CARD.replace("= card {", "= card.filter(|_| !self.cancelling) {"))]),
    ("1-3: the prompts numbered below a start not swept",
     [(SHELL, SWEEP, "")]),
    ("1-3: a start takes the oldest entry, whatever its number (the base)",
     [(SHELL, SWEEP, ""), (SHELL, TAKE, "                } = self.prompts.pop_front().unwrap_or_default();\n")]),
    ("1-3: a covered prompt still waiting (the base's stale entry)",
     [(SHELL, WAITING, "            .filter(move |_| through.is_none() || through.is_some())\n")]),
    ("1-3: a turn's stop records no count",
     [(SHELL, TURN_STOP, TURN_STOP.replace("        self.cancel_through = Some(through);\n", ""))]),
    ("1-3: a compaction's stop records no count",
     [(SHELL, COMPACTION_STOP, COMPACTION_STOP.replace("        self.cancel_through = Some(through);\n", ""))]),
    ("1-3: the band numbers a prompt before its submit",
     [(SHELL, NUMBER, "                let submission = self.work.accepted().saturating_sub(1);\n")]),
    ("1-3: a handed-on steer unnumbered",
     [(SHELL, HANDED, "                        submission: 0,\n")]),
    ("1-3: the runtime's number one past the item",
     [(WORKER, STARTED, "        submission: queue.taken + 1,\n")]),
    ("1-3: the runtime counts its turns, not the work it took",
     [(WORKER, STARTED, "        submission: 1,\n")]),
    # Item 4: a prompt queued at once.
    ("4: a prompt queued at once echoed at the keystroke (the base)",
     [(SHELL, HOLD, "                let (card, echoed) = if waits {\n")]),
    ("4: a held card shown waiting in the band",
     [(SHELL, SHOWN, "            .filter(|_| true)\n")]),
    # Item 5: a stop while one is requested.
    ("5: no guard (the base)",
     [(SHELL, GUARD, "")]),
    ("5: a manual compaction typed since held by the guard",
     [(SHELL, GUARD, GUARD.replace("!manual && ", ""))]),
    ("5: a provider switch typed since held by the guard",
     [(SHELL, GUARD, GUARD.replace(" && !self.work.switch_pending()", ""))]),
    ("5: a later turn's start does not take the stop back",
     [(SHELL, UNCOVERED, UNCOVERED.replace("                    self.stop_requested = false;\n", ""))]),
    ("5: nothing in hand does not take the stop back",
     [(SHELL, SETTLE, "")]),
    ("5: a turn's stop not recorded as requested",
     [(SHELL, TURN_STOP, TURN_STOP.replace("        self.stop_requested = true;\n", ""))]),
    ("5: a compaction's stop not recorded as requested",
     [(SHELL, COMPACTION_STOP, COMPACTION_STOP.replace("        self.stop_requested = true;\n", ""))]),
    # Item 6: the leading title.
    ("6: the lead taken from whatever title is owed (the base)",
     [(SHELL, LEAD, "                        if quiet && ran && !noted {\n                            let _ = self.title_session();\n                            self.title_leads = self.title_owed;\n")]),
    ("6: a later title keeps the stopped end's lead",
     [(SHELL, RETITLE, "        self.title_owed = true;\n")]),
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
        lines.append("%2d  SKIP      %-72s %s: the original occurs %d times" % (number, title, bad[0][0], texts[bad[0][0]].count(bad[0][1])))
        print(lines[-1], flush=True)
        continue
    for path, old, new in edits:
        texts[path] = texts[path].replace(old, new)
    for path, text in texts.items():
        open(os.path.join(clone, path), "w", encoding="utf-8").write(text)
    try:
        run = subprocess.run(TEST, cwd=clone, env=env, capture_output=True, text=True, timeout=LIMIT)
        hung = False
    except subprocess.TimeoutExpired:
        run, hung = None, True
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, env=env, check=True)
    if hung:
        lines.append("%2d  %-9s %-72s %s" % (number, "KILLED", title, "(hang: past %d s)" % LIMIT))
        print(lines[-1], flush=True)
        continue
    tail = (run.stdout + run.stderr).strip().splitlines()
    failed = [l for l in tail if l.startswith("test ") and l.endswith("FAILED")]
    err = [l for l in tail if l.startswith("error") and "could not compile" not in l][:1]
    built = not any(l.startswith("error[") or "could not compile" in l for l in tail)
    verdict = "SURVIVED" if run.returncode == 0 else ("KILLED" if failed else ("NOCOMPILE" if not built else "KILLED"))
    detail = "; ".join(f[5:-10] for f in failed[:4]) or (err[0] if err else "exit %d" % run.returncode)
    if failed and len(failed) > 4:
        detail += "; +%d more" % (len(failed) - 4)
    lines.append("%2d  %-9s %-72s %s" % (number, verdict, title, detail))
    print(lines[-1], flush=True)
open(out, "w").write("\n".join(lines) + "\n")
