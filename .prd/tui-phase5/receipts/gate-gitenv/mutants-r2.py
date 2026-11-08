#!/usr/bin/env python3
"""GATE-GITENV round 2's mutants: each round-2 fix taken out, one at a time, in
a --no-local clone (never the worktree), and scripts/test-gate-env.sh run. A
mutant is CAUGHT when the test fails and its failing cases are exactly the
ones that fix exists for.

usage: mutants-r2.py <clone> <log dir>
"""
import os
import pathlib
import re
import subprocess
import sys

clone = pathlib.Path(sys.argv[1])
logs = pathlib.Path(sys.argv[2])
logs.mkdir(parents=True, exist_ok=True)
test = clone / "scripts/test-gate-env.sh"

DROP = 'for name in "${!GIT_@}"; do case "$name" in *[!A-Za-z0-9_]*) ;; *) unset "$name" ;; esac; done\n'


def step_without_drop(workflow, step_name):
    """The workflow with the GIT_* drop taken out of one named step."""

    def edit(text):
        at = text.index("- name: " + step_name)
        nxt = text.find("\n      - ", at + 1)
        nxt = len(text) if nxt < 0 else nxt
        body = text[at:nxt]
        lines = [l for l in body.splitlines(True) if DROP.strip() in l]
        if len(lines) != 1:
            raise SystemExit(f"{workflow} {step_name}: the drop occurs {len(lines)} times, expected once")
        return text[:at] + body.replace(lines[0], "", 1) + text[nxt:]

    return edit


def replace(old, new, count=1):
    def edit(text):
        if text.count(old) != count:
            raise SystemExit(f"{old!r} occurs {text.count(old)} times, expected {count}")
        return text.replace(old, new)

    return edit


def regex(pattern, new):
    def edit(text):
        found = re.findall(pattern, text, re.S)
        if len(found) != 1:
            raise SystemExit(f"{pattern!r} matches {len(found)} times, expected once")
        return re.sub(pattern, new, text, count=1, flags=re.S)

    return edit


SEVEN = (
    "unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_OBJECT_DIRECTORY GIT_COMMON_DIR \\\n"
    "\tGIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE\n"
)

mutants = {
    # MUST 1: the entry points outside the three gates.
    "build-rs-keeps-git-env": (
        "build.rs",
        regex(r"    let mut command = Command::new\(\"git\"\);\n.*?\n    command\n", "    Command::new(\"git\")\n"),
        {"build-script"},
    ),
    "ci-assertion-keeps-git-env": (
        ".github/workflows/ci.yml",
        step_without_drop("ci.yml", "Confirm the checkout is the commit under test"),
        {"workflow-steps", "ci-checkout-assertion"},
    ),
    "sweep-build-step-keeps-git-env": (
        ".github/workflows/sweep.yml",
        step_without_drop("sweep.yml", "Build xfx at the base and at the head"),
        {"workflow-steps"},
    ),
    "test-check-no-secrets-seven-names": (
        "scripts/test-check-no-secrets.sh",
        regex(r'for name in "\$\{!GIT_@\}"; do\n.*?\ndone\n', SEVEN),
        {"test-check-no-secrets"},
    ),
    "test-check-no-secrets-reads-home": (
        "scripts/test-check-no-secrets.sh",
        replace('\tGIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"\n', '\tgit "$@"\n'),
        {"test-check-no-secrets"},
    ),
    # MUST 2: identity fails closed when git cannot finish a scan.
    "identity-listing-ignores-errors": (
        "scripts/check-xfx-identity.sh",
        replace(
            '\tif [ "$status" -gt 1 ] || [ -s "$errors" ]; then\n'
            "\t\tprintf 'check-xfx-identity: git grep could not search %s (status %d):\\n' \"$root\" \"$status\" >&2\n",
            "\tif false; then\n"
            "\t\tprintf 'check-xfx-identity: git grep could not search %s (status %d):\\n' \"$root\" \"$status\" >&2\n",
        ),
        {"identity-grep.patternType", "identity-grep.extendedRegexp", "identity-grep.threads", "identity-unreadable"},
    ),
    "identity-names-ignore-errors": (
        "scripts/check-xfx-identity.sh",
        replace('\tif [ "$status" -ne 0 ] || [ -s "$errors" ]; then\n', "\tif false; then\n"),
        {"identity-core.quotePath-home"},
    ),
    "identity-count-ignores-errors": (
        "scripts/check-xfx-identity.sh",
        replace(
            'if ! scan_git . ls-files -v -z >"$control_root/tracked" 2>"$control_root/errors" || [ -s "$control_root/errors" ]; then\n',
            'if ! scan_git . ls-files -v -z >"$control_root/tracked" 2>"$control_root/errors" && false; then\n',
        ),
        {"identity-index-corrupt"},
    ),
    "identity-reads-flagged-entries": (
        "scripts/check-xfx-identity.sh",
        replace('if [ "${#flagged[@]}" -ne 0 ]; then\n', "if false; then\n"),
        {"identity-skip-worktree", "identity-assume-unchanged"},
    ),
    # MUST 3: config the GIT_* drop leaves.
    "identity-reads-config-worktree": (
        "scripts/check-xfx-identity.sh",
        replace('\tgit -c core.fsmonitor=false -C "$root" --work-tree=. "$@"\n', '\tgit -c core.fsmonitor=false -C "$root" "$@"\n'),
        {"identity-core.worktree"},
    ),
    "identity-scan-runs-fsmonitor": (
        "scripts/check-xfx-identity.sh",
        replace('\tgit -c core.fsmonitor=false -C "$root" --work-tree=. "$@"\n', '\tgit -C "$root" --work-tree=. "$@"\n'),
        {"identity-home-programs"},
    ),
    "identity-controls-read-home": (
        "scripts/check-xfx-identity.sh",
        replace('\tGIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"\n', '\tgit "$@"\n'),
        # The pin also lets the controls start under a HOME whose config git
        # cannot parse: without it their `init` dies, exit 128, before the
        # path scan can say it could not run.
        {"identity-home-programs", "identity-core.quotePath-home"},
    ),
    "no-secrets-listing-runs-fsmonitor": (
        "scripts/check-no-secrets.sh",
        replace('git -c core.fsmonitor=false -C "$dir"', 'git -C "$dir"', count=3),
        # test-check-no-secrets runs check-no-secrets.sh under the same HOME.
        {"no-secrets-home-programs", "test-check-no-secrets"},
    ),
    "no-secrets-sample-reads-home": (
        "scripts/check-no-secrets.sh",
        regex(
            r"\texport GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1\n(.*?)git -c core\.fsmonitor=false -C \"\$sample_dir\" init(.*?)git -c core\.fsmonitor=false -C \"\$sample_dir\" add",
            r'\1git -C "$sample_dir" init\2git -C "$sample_dir" add',
        ),
        # test-check-no-secrets runs check-no-secrets.sh under the same HOME.
        {"no-secrets-home-programs", "test-check-no-secrets"},
    ),
    "preview-epoch-gate-reads-home": (
        "scripts/check-preview-contract.sh",
        replace('\t\t\t\tPATH="$work/pinned-bin:$PATH" \\\n', ""),
        {"preview-home-programs"},
    ),
}

env = dict(os.environ)
for key in list(env):
    if key.startswith("GIT_") or key == "GREP_OPTIONS":
        env.pop(key)


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as out:
        status = subprocess.run(["timeout", "1500", str(test)], cwd=clone, env=env,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT).returncode
    text = log.read_text(errors="replace")
    failing = {m.group(1) for m in re.finditer(r"^test-gate-env: ([a-zA-Z.-]+): ", text, re.M)}
    return status, failing


only = set(sys.argv[3:])
status, failing = run("control")
print(f"control: test exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)
if status != 0:
    sys.exit("the unmodified clone fails the test; no mutant below would prove anything")

caught = 0
chosen = {k: v for k, v in mutants.items() if not only or k in only}
for name, (rel, edit, expected) in chosen.items():
    path = clone / rel
    original = path.read_text()
    mutated = edit(original)
    if mutated == original:
        sys.exit(f"{name}: the edit changed nothing")
    try:
        path.write_text(mutated)
        status, failing = run(name)
    finally:
        path.write_text(original)
    verdict = "CAUGHT" if status != 0 and failing == expected else ("SURVIVED" if status == 0 else "OTHER")
    caught += verdict == "CAUGHT"
    print(f"{verdict} {name}: test exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)

print(f"{caught} of {len(chosen)} mutants caught, each by exactly its own cases")
sys.exit(0 if caught == len(chosen) else 1)
