#!/usr/bin/env python3
"""LOCAL-PATHS-EXIT-TRAP's mutants: each fix line reverted alone, then the test
that holds it run.

    mutants.py <throwaway clone> <group> [<mutant id>...]

<group> is `gate-env` (the mutants test-gate-env.sh holds, each a full run of
it) or `rest`. Each mutant is one textual change to one script in the clone;
its killer then runs with /bin/bash: the host test that holds the script's
early-stop cases, or, for a dropped `finished=1`, the script's own normal run
too. The script is restored after each. Run it on a `git clone --no-local`
only: a mutant is the script without one of its guards.

One line per mutant: killed or SURVIVED, the killer's exit and its failing
early-stop cases. No path is printed.
"""
import os
import re
import subprocess
import sys

clone = os.path.abspath(sys.argv[1])
group = sys.argv[2]
only = set(sys.argv[3:])

GUARD = """trap 'stopped=$?; rm -rf "$%s"; [ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"' EXIT"""
KEEP = 'exit "$((stopped == 0 ? 1 : stopped))"'

def one_trap(prefix, script, var, signals_were_trapped, host, own):
    guard = GUARD % var
    rows = [
        (prefix + "1", script, "the original trap back",
         guard, """trap 'rm -rf "$%s"' EXIT%s""" % (var, " HUP INT TERM" if signals_were_trapped else ""), host),
    ]
    if signals_were_trapped:
        rows += [
            (prefix + "2", script, "the guard dropped, the signals untrapped",
             guard, """trap 'rm -rf "$%s"' EXIT""" % var, host),
            (prefix + "3", script, "the guard kept, the signals trapped again", guard, guard + " HUP INT TERM", host),
        ]
    rows += [
        (prefix + "4", script, "the status not kept (exit 1)", KEEP, "exit 1", host),
        (prefix + "5", script, "finished=0 dropped", "finished=0\n" + guard, guard, host),
        (prefix + "6", script, "finished=1 dropped", "\nfinished=1\n", "\n", host + own),
    ]
    return rows

MUTANTS = []
# check-no-secrets.sh, held by test-check-no-secrets.sh.
S_HOST = ["test-check-no-secrets.sh"]
MUTANTS += [
    ("S1", "check-no-secrets.sh", "the original trap back", "trap on_exit EXIT\n", "trap 'rm -f \"$listing\"' EXIT\n", S_HOST),
    ("S2", "check-no-secrets.sh", "the guard dropped", '\t[ "$finished" = 1 ] || ' + KEEP + "\n", "", S_HOST),
    ("S3", "check-no-secrets.sh", "the status not kept (exit 1)", KEEP, "exit 1", S_HOST),
    ("S4", "check-no-secrets.sh", "no status read (local stopped=$? dropped)", "\tlocal stopped=$?\n", "", S_HOST),
    ("S5", "check-no-secrets.sh", "finished=0 dropped", "finished=0\non_exit() {", "on_exit() {", S_HOST),
    ("S6", "check-no-secrets.sh", "finished=1 dropped", "\nfinished=1\n", "\n", S_HOST),
    ("S7", "check-no-secrets.sh", "the control file's trap back",
     'control="$(mktemp "${TMPDIR:-/tmp}/xfx-secret-control.XXXXXX")"\n',
     'control="$(mktemp "${TMPDIR:-/tmp}/xfx-secret-control.XXXXXX")"\ntrap \'rm -f "$control" "$listing"\' EXIT\n', S_HOST),
    ("S8", "check-no-secrets.sh", "the sample directory's trap back",
     'sample_dir="$(mktemp -d "${TMPDIR:-/tmp}/xfx-secret-names.XXXXXX")"\n',
     'sample_dir="$(mktemp -d "${TMPDIR:-/tmp}/xfx-secret-names.XXXXXX")"\n'
     'trap \'rm -f "$control" "$listing"; rm -rf "$sample_dir"\' EXIT\n', S_HOST),
    ("S9", "check-no-secrets.sh", "the control file not removed", 'rm -f "$listing" ${control:+"$control"}', 'rm -f "$listing"', S_HOST),
    ("S10", "check-no-secrets.sh", "the sample directory not removed",
     '\tif [ -n "${sample_dir:-}" ]; then\n\t\trm -rf "$sample_dir"\n\tfi\n', "", S_HOST),
    ("S11", "check-no-secrets.sh", "the signals trapped", "trap on_exit EXIT\n", "trap on_exit EXIT HUP INT TERM\n", S_HOST),
]
MUTANTS += one_trap("P", "check-preview-contract.sh", "work", True, ["test-preview-contract.sh"], [])
MUTANTS += one_trap("T", "test-check-no-secrets.sh", "work", True, ["test-gate-env.sh"], [])
MUTANTS += one_trap("X", "check-xfx-identity.sh", "control_root", False, ["test-gate-env.sh"], ["check-xfx-identity.sh"])
MUTANTS += one_trap("R", "test-preview-contract.sh", "work", True, ["test-gate-env.sh"], ["test-preview-contract.sh"])
MUTANTS += one_trap("G", "test-gate-env.sh", "work", True, ["test-early-stop.sh"], ["test-gate-env.sh"])
# smoke.sh, held by test-early-stop.sh; its normal run needs a built binary,
# named by XFX_SMOKE_BINARY.
M_HOST = ["test-early-stop.sh"]
MUTANTS += [
    ("M1", "smoke.sh", "the original cleanup back",
     'cleanup() {\n\tlocal stopped=$?\n\tstop_gateway\n\t[ "$finished" = 1 ] || ' + KEEP + "\n}\n",
     "cleanup() {\n\tstop_gateway\n}\n", M_HOST),
    ("M2", "smoke.sh", "the guard dropped", '\t[ "$finished" = 1 ] || ' + KEEP + "\n", "", M_HOST),
    ("M3", "smoke.sh", "the status not kept (exit 1)", KEEP, "exit 1", M_HOST),
    ("M4", "smoke.sh", "no status read (local stopped=$? dropped)", "\tlocal stopped=$?\n", "", M_HOST),
    ("M5", "smoke.sh", "finished=0 dropped", "finished=0\ncleanup() {", "cleanup() {", M_HOST),
    ("M6", "smoke.sh", "finished=1 dropped", "\nfinished=1\n", "\n", M_HOST + ["smoke.sh"]),
]

def wanted(m):
    in_group = "test-gate-env.sh" in m[5] and m[1] != "test-gate-env.sh"
    if group == "gate-env" and not in_group:
        return False
    if group == "rest" and in_group:
        return False
    return not only or m[0] in only

def run(killer):
    argv = ["/bin/bash", "scripts/" + killer]
    if killer == "smoke.sh":
        argv += [os.environ["XFX_SMOKE_BINARY"], os.path.join(os.environ["TMPDIR"], "mutant-smoke")]
    out = subprocess.run(argv, cwd=clone, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT)
    text = out.stdout.decode("utf-8", "replace")
    failing = sorted({m.group(1) for m in re.finditer(r"([a-z0-9-]*stopped-[a-z0-9]+)", text)
                      if not re.search(r"(ok|caught)\s+" + re.escape(m.group(1)) + r"\s", text)})
    return out.returncode, failing

for m in MUTANTS:
    if not wanted(m):
        continue
    mid, script, what, old, new, killers = m
    path = os.path.join(clone, "scripts", script)
    pristine = open(path).read()
    if pristine.count(old) != 1:
        print(f"{mid} {script}: {what}: SKIPPED, the text to change is there {pristine.count(old)} time(s)", flush=True)
        continue
    with open(path, "w") as f:
        f.write(pristine.replace(old, new))
    results = []
    killed = False
    try:
        for killer in killers:
            code, failing = run(killer)
            # A killer that is the script itself (its normal run) kills by any
            # failure; a host kills by failing.
            results.append(f"{killer} exit {code}" + (f", failing {', '.join(failing)}" if failing else ""))
            if code != 0:
                killed = True
    finally:
        with open(path, "w") as f:
            f.write(pristine)
    print(f"{mid} {script}: {what}: {'killed' if killed else 'SURVIVED'} ({'; '.join(results)})", flush=True)
clean = subprocess.run(["git", "status", "--porcelain"], cwd=clone, stdout=subprocess.PIPE).stdout.strip()
print(f"# clone clean after: {'yes' if not clean else 'no'}")
