#!/usr/bin/env python3
"""GATE-GITENV round 2d's mutants: round 2's to 2d's fixes, each taken out, one at a time, in
a --no-local clone (never the worktree), and scripts/test-gate-env.sh run. A
mutant is CAUGHT when the test fails and its failing cases are exactly the
ones that fix exists for.

usage: mutants-r2d.py <clone> <log dir> [mutant...]

The miscased cases run only where the file system folds case; elsewhere
they print a skip line, so a mutant only they can catch is skipped there,
and they are left out of every expected set.
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
        {"build-script", "build-script-linked-worktree"},
    ),
    # 2b: what build.rs watches in a linked worktree.
    "build-rs-watches-the-main-head": (
        "build.rs",
        replace('    let head = git_dir.join("HEAD");\n', '    let head = common.join("HEAD");\n'),
        {"build-script-linked-worktree"},
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
        replace('if [ "${#skipped[@]}" -ne 0 ] || [ "${#assumed[@]}" -ne 0 ]; then\n', "if false; then\n"),
        # No refusal at all fails the both-bits case too.
        {"identity-skip-worktree", "identity-assume-unchanged", "identity-both-bits"},
    ),
    # 2b: the remedy a sparse checkout is given.
    "identity-sparse-remedy-clears-the-bit": (
        "scripts/check-xfx-identity.sh",
        replace("restore them first: in a sparse checkout, git sparse-checkout disable; for a file the working tree holds, ",
                "clear the bit: "),
        # identity-both-bits requires the sparse remedy too, since 2d.
        {"identity-skip-worktree", "identity-both-bits"},
    ),
    # 2c/2d: a copy inside another repository, by any path: its .git not
    # opening, ignored. (The -ef comparison then refuses it, by the other
    # message.)
    "identity-own-git-failure-ignored": (
        "scripts/check-xfx-identity.sh",
        replace("""if ! own="$(git -c core.fsmonitor=false --git-dir="$repo_root/.git" rev-parse --absolute-git-dir 2>"$control_root/errors")"; then
""", """if ! own="$(git -c core.fsmonitor=false --git-dir="$repo_root/.git" rev-parse --absolute-git-dir 2>"$control_root/errors" || true)"; then
"""),
        {"identity-vendored", "identity-vendored-symlink", "identity-vendored-colon", "~identity-vendored-miscased"},
    ),
    # 2d: git's reason for the root's .git not opening, dropped.
    "identity-own-git-reason-dropped": (
        "scripts/check-xfx-identity.sh",
        replace("""rev-parse --absolute-git-dir 2>"$control_root/errors")"; then
	fail "not a git checkout of its own: the root's .git cannot be opened""", """rev-parse --absolute-git-dir 2>/dev/null)"; then
	: >"$control_root/errors"
	fail "not a git checkout of its own: the root's .git cannot be opened"""),
        {"identity-vendored", "identity-vendored-symlink", "identity-vendored-colon", "~identity-vendored-miscased"},
    ),
    # 2c: round 2b's fix, a ceiling at the parent directory, in place of the
    # check (with 2d's message, so only the paths it missed fail).
    "identity-ceiling-instead": (
        "scripts/check-xfx-identity.sh",
        regex(
            r"if ! discovered=\"\$\(scan_git \. rev-parse --absolute-git-dir 2>\"\$control_root/errors\"\)\"; then\n.*?\nfi\nif ! own=.*?\nfi\nif ! \[ \"\$discovered\" -ef \"\$own\" \]; then\n.*?\nfi\n",
            """if ! GIT_CEILING_DIRECTORIES="$(dirname "$repo_root")" scan_git . rev-parse --git-dir \\\\
	>/dev/null 2>"$control_root/errors"; then
	fail "not a git checkout of its own: the root's .git cannot be opened, or git cannot open it:"
	cat "$control_root/errors" >&2
	exit 1
fi
""",
        ),
        # The one message carries what identity-vendored and
        # identity-core.quotePath each require, as 2b's single message did.
        {"identity-vendored-symlink", "identity-vendored-colon", "~identity-vendored-miscased"},
    ),
    # 2c: the two repositories compared as paths, not as files.
    "identity-own-check-by-path": (
        "scripts/check-xfx-identity.sh",
        replace('if ! [ "$discovered" -ef "$own" ]; then\n', 'if [ "$discovered" != "$own" ]; then\n'),
        {"~identity-own-miscased"},
    ),
    # 2d: tracked files missing from the working tree, scanned as if there.
    "identity-skips-missing-files": (
        "scripts/check-xfx-identity.sh",
        replace('if [ -s "$control_root/deleted" ]; then\n', "if false; then\n"),
        {"identity-vendored-gitfile", "identity-vendored-git-symlink", "identity-deleted-file"},
    ),
    # 2d: the missing-file listing's failure, ignored.
    "identity-missing-listing-ignores-errors": (
        "scripts/check-xfx-identity.sh",
        replace("""if ! scan_git . ls-files -d -z >"$control_root/deleted" 2>"$control_root/errors" || [ -s "$control_root/errors" ]; then
""", """if ! scan_git . ls-files -d -z >"$control_root/deleted" 2>"$control_root/errors"; then
"""),
        {"identity-unsearchable-dir"},
    ),
    # 2c: an entry with both bits, in one list only.
    "identity-both-bits-one-list": (
        "scripts/check-xfx-identity.sh",
        replace("""		skipped+=("${entry#??}")
		assumed+=("${entry#??}")
""", """		skipped+=("${entry#??}")
"""),
        {"identity-both-bits"},
    ),
    # 2d: the other half: an entry with both bits in the assume-unchanged
    # list only.
    "identity-both-bits-other-list": (
        "scripts/check-xfx-identity.sh",
        replace("""		skipped+=("${entry#??}")
		assumed+=("${entry#??}")
""", """		assumed+=("${entry#??}")
"""),
        {"identity-both-bits"},
    ),
    # 2b: a fix for that which would refuse a linked worktree, whose .git is
    # a file.
    "identity-requires-a-git-directory": (
        "scripts/check-xfx-identity.sh",
        replace("# --- the scan itself ---------------------------------------------------------\n",
                "# --- the scan itself ---------------------------------------------------------\n"
                "[ -d .git ] || { fail \"not a git checkout of its own: the root's .git cannot be opened: not a git repository (no .git directory)\"; exit 1; }\n"),
        # With the message the vendored cases require, so only a .git that is a
        # file is refused wrongly: a linked worktree's, a submodule's, and the
        # planted gitfile (refused, but not as missing files).
        {"identity-linked-worktree", "identity-submodule", "identity-vendored-gitfile"},
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

# Where the file system folds case, the "~" cases run; elsewhere they skip.
folds = os.path.exists(str(clone).rstrip("/").upper()) and os.path.samefile(clone, str(clone).rstrip("/").upper())
for name, (rel, edit, expected) in list(mutants.items()):
    kept = {c.lstrip("~") for c in expected if folds or not c.startswith("~")}
    mutants[name] = (rel, edit, kept)
print(f"this file system {'folds' if folds else 'does not fold'} case", flush=True)

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
    if not expected:
        print(f"SKIP {name}: only the miscased cases catch it, and they do not run here", flush=True)
        continue
    try:
        path.write_text(mutated)
        status, failing = run(name)
    finally:
        path.write_text(original)
    verdict = "CAUGHT" if status != 0 and failing == expected else ("SURVIVED" if status == 0 else "OTHER")
    caught += verdict == "CAUGHT"
    print(f"{verdict} {name}: test exit {status}; failing: {', '.join(sorted(failing)) or '-'}", flush=True)

ran = sum(1 for _, (_, _, e) in chosen.items() if e)
print(f"{caught} of {ran} mutants caught, each by exactly its own cases")
sys.exit(0 if caught == ran else 1)
