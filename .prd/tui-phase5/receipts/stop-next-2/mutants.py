"""STOP-NEXT-2's code mutants, run on a --no-local clone (never the worktree).

usage: python3 -I -B mutants.py <clone> <target-dir> <out-file> [n,n,...]

STOP-NEXT's runner (`../stop-next/mutants.py`) with this unit's rows; a
per-mutant time limit: a mutant whose suite runs past it is
KILLED (hang). Each mutant is one exact replacement (or two) in
src/tui/shell.rs or src/tui/event_loop.rs; the original must occur exactly once,
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
LOOP = "src/tui/event_loop.rs"


TYPED = (
    "        let compaction = self.compaction.as_ref().is_some_and(|compaction| {\n"
    "            compaction.active() && !compaction.stopped() && !compaction.automatic()\n"
    "        });\n"
)
SWITCH = "            .is_some_and(|at| !self.cancel_through.is_some_and(|through| at <= through));\n"
LEAD_STOP = (
    "        let lead = self.title_leads.take()?;\n"
    "        if self.stop_in_tick {\n"
    "            self.title_held = true;\n"
    "            return None;\n"
    "        }\n"
)
LEAD_OTHER = (
    "        if now == lead {\n"
    "            self.title_owed = false;\n"
    "        } else {\n"
    "            self.title_held = true;\n"
    "        }\n"
)
TURN_STOP = (
    "        self.cancel_through = Some(through);\n"
    "        self.stop_requested = true;\n"
    "        self.stop_in_tick = true;\n"
    "        // The turn this stops ends with no bell"
)
COMPACTION_STOP = (
    "        self.stop_requested = true;\n"
    "        self.stop_in_tick = true;\n"
    "        let automatic"
)
TAKE = "        if self.title_held || !std::mem::take(&mut self.title_owed) {\n"
OWES = "        self.title_owed && !self.title_held\n"
COMMITTED = (
    "        self.stop_in_tick = false;\n"
    "        if std::mem::take(&mut self.title_held) {\n"
    "            self.render.request(Reason::Footer);\n"
    "        }\n"
)
HOOK = "    shell.tick_committed();\n"
KEEP = "                } else if self.keeps_late_text() {\n                    self.late.push((false, text));\n"
PRESENT = (
    "                for (operational, text) in std::mem::take(&mut self.late) {\n"
    "                    if quiet {\n"
    "                        break;\n"
    "                    }\n"
)
EMPTIED = "        if emptied {\n            self.late.clear();\n        }\n"
DRAINED = "            Err(_) => return true,\n"
CAP = "    }\n    false\n}\n\n/// The three things an exit borrows"
BACKLOG = "        self.pacer.pending().saturating_add(late)\n"

# Each mutant: a title and its edits, (path, original, replacement).
MUTANTS = [
    # R1: the exemption.
    ("R1: the stop's own compaction exempt while it unwinds (the base)",
     [(SHELL, TYPED, TYPED.replace(" && !compaction.stopped()", ""))]),
    ("R1: every pending switch exempt (the base)",
     [(SHELL, SWITCH, "            .is_some();\n")]),
    ("R1: a switch the stop covers by one exempt (off by one)",
     [(SHELL, SWITCH, SWITCH.replace("at <= through", "at < through"))]),
    # Item 7 and one title per tick.
    ("7: a stop's own tick leads with the end's title (the base)",
     [(SHELL, LEAD_STOP, "        let lead = self.title_leads.take()?;\n")]),
    ("7: a turn's stop marks no tick",
     [(SHELL, TURN_STOP, TURN_STOP.replace("        self.stop_in_tick = true;\n", ""))]),
    ("7: a compaction's stop marks no tick",
     [(SHELL, COMPACTION_STOP, COMPACTION_STOP.replace("        self.stop_in_tick = true;\n", ""))]),
    ("T1: a later title replaces the end's in the same frame (the base)",
     [(SHELL, LEAD_OTHER, "        self.title_owed = now != lead;\n")]),
    ("T1, 7: a held title is taken anyway",
     [(SHELL, TAKE, "        if !std::mem::take(&mut self.title_owed) {\n")]),
    ("T1, 7: a held title still counts as this tick's",
     [(SHELL, OWES, "        self.title_owed\n")]),
    ("T1, 7: no frame asked for a held title",
     [(SHELL, COMMITTED, "        self.stop_in_tick = false;\n        self.title_held = false;\n")]),
    ("T1, 7: a stop's tick never ends",
     [(SHELL, COMMITTED, COMMITTED.replace("        self.stop_in_tick = false;\n", ""))]),
    ("T1, 7: the loop never ends a tick",
     [(LOOP, HOOK, "")]),
    # Item 8: late text.
    ("8: late text never kept (the base)",
     [(SHELL, KEEP, "                } else if false {\n                    self.late.push((false, text));\n")]),
    ("8: late text presented under the stop's own end",
     [(SHELL, PRESENT, PRESENT.replace("if quiet {", "if false {"))]),
    ("8: an emptied drain keeps late text",
     [(SHELL, EMPTIED, "        let _ = emptied;\n")]),
    ("8: every drain drops late text",
     [(SHELL, EMPTIED, "        let _ = emptied;\n        self.late.clear();\n")]),
    ("8: a drain cut at its bound counts as emptied",
     [(LOOP, CAP, CAP.replace("    false\n}", "    true\n}"))]),
    ("8: late text outside the drain's bound",
     [(SHELL, BACKLOG, "        let _ = late;\n        self.pacer.pending()\n")]),
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
