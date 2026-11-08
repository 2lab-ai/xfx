#!/usr/bin/env python3
"""Mutants of the COMPACT-NOTICE-LATE fix, each on a throwaway clone.

usage: python3 -I mutants.py <clone-dir> <out-file>
The clone is a `git clone --no-local` of the unit's head. Each mutant is a
string replacement in one file; the clone is reset (`git checkout -- .`)
before each. Every mutant runs the lib tests matching compact/stop/cancel
and the pty tests matching compact, each under a time limit; a mutant is
killed when any test fails (or the build fails), and survives otherwise.
"""
import subprocess, sys, os

clone, out = sys.argv[1], sys.argv[2]
SHELL = "src/tui/shell.rs"

TAKE = """        let idle = self.work.outstanding() == 0;
        if drain(self) {
            self.release_late(false);
            if idle {
                self.settle_unstarted_compaction();
            }
        }"""
SETTLE_BAND = """        // A compaction the user stopped is not over here, even with nothing in
        // hand: its ending can still be on the channel
        // ([`Self::settle_unstarted_compaction`]).
"""
MUTANTS = [
    ("M1 no idle gate", SHELL, TAKE, """        if drain(self) {
            self.release_late(false);
            self.settle_unstarted_compaction();
        }"""),
    ("M2 no emptied gate", SHELL, TAKE, """        let idle = self.work.outstanding() == 0;
        if drain(self) {
            self.release_late(false);
        }
        if idle {
            self.settle_unstarted_compaction();
        }"""),
    ("M3 count read after the drain", SHELL, TAKE, """        let emptied = drain(self);
        let idle = self.work.outstanding() == 0;
        if emptied {
            self.release_late(false);
            if idle {
                self.settle_unstarted_compaction();
            }
        }"""),
    ("M4 the base's settle-time inference back", SHELL, SETTLE_BAND,
     """        if self.work.outstanding() == 0 {
            self.settle_unstarted_compaction();
        }
"""),
    ("M5 no inference at all", SHELL, """            if idle {
                self.settle_unstarted_compaction();
            }""", """            let _ = idle;"""),
    ("M6 settle any running one, not only a stopped one", SHELL,
     """            .filter(|compaction| compaction.stopped())""",
     """            .filter(|compaction| compaction.active())"""),
    ("M7 no program status for it", SHELL, """        if !compaction.automatic() {
            self.program_status""", """        if false && !compaction.automatic() {
            self.program_status"""),
]

env = dict(os.environ, TMPDIR="/private/tmp/claude-501/cnl", CARGO_TARGET_DIR=os.path.join(clone, "target"))

def run(cmd, limit):
    try:
        p = subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True, timeout=limit)
        return p.returncode, p.stdout + p.stderr
    except subprocess.TimeoutExpired as e:
        return "timeout", ""

def failing(text):
    return [l for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")][:6]

with open(out, "w") as f:
    head = subprocess.run(["git", "rev-parse", "--short=8", "HEAD"], cwd=clone, capture_output=True, text=True).stdout.strip()
    f.write(f"# mutants of the fix, on a clone at {head}\n")
    for name, path, old, new in MUTANTS:
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
        src = open(os.path.join(clone, path)).read()
        if src.count(old) != 1:
            f.write(f"{name}: NOT APPLIED (anchor count {src.count(old)})\n"); f.flush(); continue
        open(os.path.join(clone, path), "w").write(src.replace(old, new))
        verdict, why = "survived", []
        for cmd, limit in (
            (["cargo", "test", "--locked", "--lib", "--", "compact", "stop", "cancel"], 1200),
            (["cargo", "test", "--locked", "--test", "tui", "--", "compact"], 1200),
        ):
            code, text = run(cmd, limit)
            if code == "timeout":
                verdict, why = "killed", [f"timeout in {' '.join(cmd[2:5])}"]; break
            if code != 0:
                verdict = "killed"
                why = failing(text) or [l for l in text.splitlines() if l.startswith("error")][:3]
                break
        f.write(f"{name}: {verdict}" + (": " + " | ".join(why) if why else "") + "\n"); f.flush()
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
    f.write("# done\n")
