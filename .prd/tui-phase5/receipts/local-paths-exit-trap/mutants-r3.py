#!/usr/bin/env python3
"""Round 3's mutants: the renamed markers, the unbound-variable assertion,
and the TMPDIR-empty check shown to fire, each change alone, then the tests
that hold it.

    mutants-r3.py <throwaway clone> [<mutant id>...]

Run W3 on a clone at the head and on one at 1fad3ed6 (round 2's head, before
the unbound-variable assertion): there it survives, which is the RED.

Each mutant is one textual change to one file in the clone; its killers then
run with /bin/bash, from the clone. A killer given as `<script>@<rev>` is that
script as it was at <rev> (`git show`), put in place for the run: the old
host against the same mutant. The file and the killers are restored after
each. Run it on a `git clone --no-local` only: a mutant is a file without
one of its guards.

One line per mutant: killed or SURVIVED, each killer's exit and its failing
cases. No path is printed.
"""
import os
import re
import subprocess
import sys

clone = os.path.abspath(sys.argv[1])
only = set(sys.argv[2:])

GUARD = """trap 'stopped=$?; rm -rf "$work"; [ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"' EXIT"""
KEEP = 'exit "$((stopped == 0 ? 1 : stopped))"'
SMOKE_GUARDED = ('finished=0\ncleanup() {\n\tlocal stopped=$?\n\tstop_gateway\n\t[ "$finished" = 1 ] || '
                 + KEEP + "\n}\n")

MUTANTS = [
    # check-no-secrets.sh's markers under their new names, held by
    # test-check-no-secrets.sh.
    ("SL1", "scripts/check-no-secrets.sh", "listing_done=1 dropped", "\tlisting_done=1\n", "", ["test-check-no-secrets.sh"]),
    ("SL2", "scripts/check-no-secrets.sh", "the listing marker not checked",
     'if [ "$listing_done" != 1 ]; then', "if false; then", ["test-check-no-secrets.sh"]),
    ("SS1", "scripts/check-no-secrets.sh", "scan_done=1 dropped", "\tscan_done=1\n", "", ["test-check-no-secrets.sh"]),
    ("SS2", "scripts/check-no-secrets.sh", "the scan marker not checked",
     'if [ "$scan_done" != 1 ]; then', "if false; then", ["test-check-no-secrets.sh"]),
    # The generator's `unset` stops by `false` instead of an unset variable:
    # status 1, its marker printed, but no unbound-variable line. Every host's
    # unset cases must fail it.
    ("W3", "scripts/early-stop-env.sh", "an unset stop by `false`, not an unset variable",
     """unset) body=': "$xfx_test_unset"' ;;""", "unset) body='false' ;;",
     ["test-early-stop.sh", "test-preview-contract.sh", "test-check-no-secrets.sh", "test-gate-env.sh"]),
    # A subject's trap that leaves its scratch directory: the TMPDIR-empty
    # check must fire.
    ("P7", "scripts/check-preview-contract.sh", "its trap does not remove its scratch directory",
     """trap 'stopped=$?; rm -rf "$work"; [""", """trap 'stopped=$?; [""", ["test-preview-contract.sh"]),
    ("X7", "scripts/check-xfx-identity.sh", "its trap does not remove its scratch directory",
     """trap 'stopped=$?; rm -rf "$control_root"; [""", """trap 'stopped=$?; [""", ["test-gate-env.sh"]),
    ("G7", "scripts/test-gate-env.sh", "its trap does not remove its scratch directory",
     """trap 'stopped=$?; rm -rf "$work"; [""", """trap 'stopped=$?; [""", ["test-early-stop.sh"]),
]


def run(killer):
    script, _, rev = killer.partition("@")
    path = os.path.join(clone, "scripts", script)
    saved = None
    if rev:
        saved = open(path).read()
        old = subprocess.run(["git", "show", "%s:scripts/%s" % (rev, script)], cwd=clone,
                             stdout=subprocess.PIPE, check=True).stdout
        with open(path, "wb") as f:
            f.write(old)
    try:
        out = subprocess.run(["/bin/bash", "scripts/" + script], cwd=clone, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    finally:
        if saved is not None:
            with open(path, "w") as f:
                f.write(saved)
    text = out.stdout.decode("utf-8", "replace")
    failing = sorted({m.group(1) for m in re.finditer(
        r"^test-[a-z-]+: ((?:[a-z0-9-]+-)?(?:stopped|skipped)-[a-z0-9-]+|ci-bash32|generator-exit-only):",
        text, re.M)})
    return out.returncode, failing


for mid, rel, what, old, new, killers in MUTANTS:
    if only and mid not in only:
        continue
    path = os.path.join(clone, rel)
    pristine = open(path).read()
    if pristine.count(old) != 1:
        print(f"{mid} {rel}: {what}: SKIPPED, the text to change is there {pristine.count(old)} time(s)", flush=True)
        continue
    with open(path, "w") as f:
        f.write(pristine.replace(old, new))
    results, killed = [], False
    try:
        for killer in killers:
            code, failing = run(killer)
            results.append(f"{killer} exit {code}" + (f", failing {', '.join(failing)}" if failing else ""))
            # The old host is the comparison, not a killer.
            if code != 0 and "@" not in killer:
                killed = True
    finally:
        with open(path, "w") as f:
            f.write(pristine)
    print(f"{mid} {rel}: {what}: {'killed' if killed else 'SURVIVED'} ({'; '.join(results)})", flush=True)
clean = subprocess.run(["git", "status", "--porcelain"], cwd=clone, stdout=subprocess.PIPE).stdout.strip()
print(f"# clone clean after: {'yes' if not clean else 'no'}")
