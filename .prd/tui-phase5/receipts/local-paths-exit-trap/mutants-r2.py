#!/usr/bin/env python3
"""Round 2's mutants: each new guard reverted alone, then the test that holds it.

    mutants-r2.py <throwaway clone> [<mutant id>...]

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
    # check-no-secrets.sh's listing and scan markers, held by
    # test-check-no-secrets.sh.
    ("SL1", "scripts/check-no-secrets.sh", "listed=1 dropped", "\tlisted=1\n", "", ["test-check-no-secrets.sh"]),
    ("SL2", "scripts/check-no-secrets.sh", "the listing marker not checked",
     'if [ "$listed" != 1 ]; then', "if false; then", ["test-check-no-secrets.sh"]),
    ("SS1", "scripts/check-no-secrets.sh", "scanned=1 dropped", "\tscanned=1\n", "", ["test-check-no-secrets.sh"]),
    ("SS2", "scripts/check-no-secrets.sh", "the scan marker not checked",
     'if [ "$scanned" != 1 ]; then', "if false; then", ["test-check-no-secrets.sh"]),
    # test-early-stop.sh's own guard, held by test-gate-env.sh.
    ("E1", "scripts/test-early-stop.sh", "its original trap back",
     GUARD, """trap 'rm -rf "$work"' EXIT""", ["test-gate-env.sh"]),
    ("E4", "scripts/test-early-stop.sh", "the status not kept (exit 1)", KEEP, "exit 1", ["test-gate-env.sh"]),
    ("E5", "scripts/test-early-stop.sh", "finished=0 dropped", "finished=0\n" + GUARD, GUARD, ["test-gate-env.sh"]),
    ("E6", "scripts/test-early-stop.sh", "finished=1 dropped", "\nfinished=1\n", "\n",
     ["test-gate-env.sh", "test-early-stop.sh"]),
    # The generator, held by test-early-stop.sh.
    ("W1", "scripts/early-stop-env.sh", "its trap stops on any trap",
     "\t\t'\tlocal name' \\\n\t\t'\tfor name in \"${@:2}\"; do' \\\n\t\t'\t\tcase \"$name\" in' \\\n"
     "\t\t'\t\tEXIT | 0)' \\\n\t\t'\t\t\txfx_early_stop' \\\n\t\t'\t\t\treturn' \\\n\t\t'\t\t\t;;' \\\n"
     "\t\t'\t\tesac' \\\n\t\t'\tdone' \\\n",
     "\t\t'\txfx_early_stop' \\\n", ["test-early-stop.sh"]),
    ("W2", "scripts/early-stop-env.sh", "the EXIT trap not printed",
     "printf '\\t%s\\n' 'builtin printf \"early stop: %s\\n\" \"$(builtin trap -p EXIT)\" >&2'\n", "",
     ["test-early-stop.sh"]),
    # A `trap '' PIPE` above smoke.sh's EXIT trap, its guard reverted: the old
    # host stopped at the first trap, before any EXIT trap, and passed.
    ("PIPE", "scripts/smoke.sh", "a trap '' PIPE above the EXIT trap, the guard reverted",
     SMOKE_GUARDED, "trap '' PIPE\ncleanup() {\n\tstop_gateway\n}\n",
     ["test-early-stop.sh", "test-early-stop.sh@ef4e74ea"]),
    # ci.yml's pin, held by test-gate-env.sh (`ci-bash32`).
    ("C1", ".github/workflows/ci.yml", "the macOS row's bash32 dropped", "            bash32: true\n", "",
     ["test-gate-env.sh"]),
    ("C2", ".github/workflows/ci.yml", "/bin/bash not put on PATH",
     '          ln -sf /bin/bash "$RUNNER_TEMP/bash32/bash"\n', "", ["test-gate-env.sh"]),
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
