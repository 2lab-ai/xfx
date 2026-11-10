#!/usr/bin/env bash
#
# Proves that scripts/check-preview-contract.sh fails what it exists to fail.
#
#   scripts/test-preview-contract.sh
#
# A contract check that passes the real workflow has only shown that it passes
# the real workflow. The checks that keep a preview per epic -- the path filter,
# the trigger set, the epoch gate in preflight and the condition on the jobs
# behind it, the epoch file itself -- and the ones that keep the tap key inside
# its `main`-only environment are each one line that can be deleted in a hurry,
# and a check that no longer notices the deletion goes on reporting `ok`.
#
# So each case below breaks one of those things in a scratch copy of the tree,
# runs the contract check against the copy, and requires exit 1 together with
# the problem that names that break: a mutant caught for some other reason is
# not proof the check for this one works. An unbroken copy has to pass first,
# or every "caught" mutant could be the harness failing everything.
#
# Requires what the contract check requires: ruby and git.

set -euo pipefail

# BSD grep -- macOS's -- puts `GREP_OPTIONS` in front of every argument list:
# a `-v` there makes "the log names this mutant's own break" true of any
# log with another line, so a mutant caught for another reason counts. (GNU grep 3.8 ignores it.) This script's options are its own;
# scripts/test-gate-env.sh holds it to that.
unset GREP_OPTIONS

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

check="$repo_root/scripts/check-preview-contract.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-preview-mutants.XXXXXX")"
trap 'rm -rf "$work"' EXIT HUP INT TERM

failures=0
caught=0

# A scratch tree holding what the contract check reads from one: every
# workflow, the epoch file and the README.
tree_for() {
	local root="$work/$1"
	mkdir -p "$root/.github/workflows"
	cp .github/workflows/*.yml "$root/.github/workflows/"
	cp .github/preview-epoch "$root/.github/preview-epoch"
	cp README.md "$root/README.md"
	printf '%s\n' "$root"
}

# One edit to the workflow text, refused unless it lands exactly once: a
# mutation that matched nothing would leave the real workflow, which passes,
# and report that as a check that failed to catch it.
mutate() {
	ruby - "$1/.github/workflows/preview.yml" "$2" <<'RUBY'
path, name = ARGV
text = File.read(path)

paths_line = "    paths: [.github/preview-epoch]\n"
job_condition = "    if: needs.preflight.outputs.publish == 'true'\n"
environment_line = "    environment: preview\n"

once = lambda do |from, to|
  count = text.scan(from).length
  abort "mutation #{name}: #{from.inspect} occurs #{count} times, expected once" unless count == 1
  text = text.sub(from) { to }
end

# The condition is the same line in both jobs; this removes the one in `job`.
unconditional = lambda do |job|
  start = text.index("\n  #{job}:\n") or abort "mutation #{name}: no #{job} job"
  finish = text.index(/\n  [a-z]+:\n/, start + 1) || text.length
  at = text.index(job_condition, start)
  abort "mutation #{name}: #{job} has no condition to remove" if at.nil? || at > finish
  text = text[0...at] + text[(at + job_condition.length)..]
end

case name
when "paths-dropped" then once.(paths_line, "")
when "paths-everything" then once.(paths_line, "    paths: [\"**\"]\n")
when "paths-renamed" then once.(paths_line, "    paths: [.github/epoch]\n")
when "tags-added" then once.(paths_line, paths_line + "    tags: [\"preview-*\"]\n")
when "dispatch-added" then once.(paths_line, paths_line + "  workflow_dispatch:\n")
when "gate-removed"
  lines = text.lines
  start = lines.index("      - name: Publish only if this push bumps the preview epoch\n") or
    abort "mutation #{name}: no epoch gate step"
  finish = (start + 1...lines.length).find { |i| lines[i].match?(/\A      (- |#)|\A  \S/) }
  text = (lines[0...start] + lines[finish..]).join
when "gate-always-publishes" then once.("if [ \"$changed\" = \"$epoch\" ]; then", "if true; then")
when "build-unconditional" then unconditional.("build")
when "publish-unconditional" then unconditional.("publish")
# The id preflight names the build with, not stamped, or stamped from
# something else: the binary would say another build's minute, or none.
when "id-unstamped" then once.("          XFX_BUILD_ID: ${{ needs.preflight.outputs.build_id }}\n", "")
when "id-from-sha" then once.("XFX_BUILD_ID: ${{ needs.preflight.outputs.build_id }}",
                              "XFX_BUILD_ID: ${{ needs.preflight.outputs.sha12 }}")
when "id-unchecked" then once.("[0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{4}-[0-9a-f]{12}", "[0-9a-z-]+")
when "environment-dropped" then once.(environment_line, "")
when "environment-renamed" then once.(environment_line, "    environment: production\n")
when "environment-on-preflight"
  once.("    name: preflight\n", "    name: preflight\n" + environment_line)
when "key-without-environment"
  once.("    timeout-minutes: 45\n",
        "    timeout-minutes: 45\n    env:\n      TAP_PUSH_KEY: ${{ secrets.TAP_PUSH_KEY }}\n")
when "build-whole-context"
  once.("    timeout-minutes: 45\n", "    timeout-minutes: 45\n    env:\n      ALL: ${{ toJSON(secrets) }}\n")
when "build-bracket-key"
  once.("    timeout-minutes: 45\n", "    timeout-minutes: 45\n    env:\n      KEY: ${{ secrets['TAP_PUSH_KEY'] }}\n")
# A `}}` inside a string literal is not the end of the expression to GitHub;
# a scanner that stops there never reaches the read that follows it.
when "build-brace-literal"
  once.("    timeout-minutes: 45\n",
        "    timeout-minutes: 45\n    env:\n      ALL: ${{ contains('}}', 'x') || toJSON(secrets) }}\n")
when "publish-action-whole-context"
  once.("      - name: Collect every asset\n",
        "      - name: Report\n        uses: actions/github-script@v7\n        with:\n" \
        "          script: ${{ toJSON(secrets) }}\n\n      - name: Collect every asset\n")
else abort "mutation #{name}: unknown"
end

File.write(path, text)
RUBY
}

# expect <case> <root> <problem the check must report>
expect() {
	local name="$1" root="$2" problem="$3" status=0
	"$check" "$root/.github/workflows/preview.yml" "$root" >"$work/$name.log" 2>&1 || status=$?
	if [ "$status" -ne 1 ]; then
		printf 'test-preview-contract: %s was not caught: the check exited %d\n' "$name" "$status" >&2
		sed 's/^/  /' "$work/$name.log" >&2
		failures=$((failures + 1))
	elif ! grep -Fq -- "$problem" "$work/$name.log"; then
		printf 'test-preview-contract: %s failed the check, but not for its own break; expected: %s\n' "$name" "$problem" >&2
		sed 's/^/  /' "$work/$name.log" >&2
		failures=$((failures + 1))
	else
		printf 'test-preview-contract: caught %-28s %s\n' "$name" "$problem"
		caught=$((caught + 1))
	fi
}

# The control: an unbroken copy passes, through the same two arguments.
control="$(tree_for control)"
if ! "$check" "$control/.github/workflows/preview.yml" "$control" >"$work/control.log" 2>&1; then
	printf 'test-preview-contract: the unbroken copy fails the check, so no mutant below proves anything\n' >&2
	sed 's/^/  /' "$work/control.log" >&2
	exit 1
fi

# --- the workflow ------------------------------------------------------------

for row in \
	"paths-dropped|the push trigger's paths are nil" \
	"paths-everything|the push trigger's paths are [\"**\"]" \
	"paths-renamed|the push trigger's paths are [\".github/epoch\"]" \
	"tags-added|the push trigger filters on [\"branches\", \"paths\", \"tags\"]" \
	"dispatch-added|the workflow triggers on [\"push\", \"workflow_dispatch\"]" \
	"gate-removed|preflight has no epoch gate" \
	"gate-always-publishes|expected skip" \
	"build-unconditional|build runs on nil" \
	"publish-unconditional|publish runs on nil" \
	"id-unstamped|the release build is not stamped with the id preflight named it through XFX_BUILD_ID" \
	"id-from-sha|the release build is not stamped with the id preflight named it through XFX_BUILD_ID" \
	"id-unchecked|preflight does not validate the build id against its exact grammar" \
	"environment-dropped|publish reads TAP_PUSH_KEY under environment nil" \
	"environment-renamed|publish reads TAP_PUSH_KEY under environment \"production\"" \
	"environment-on-preflight|preflight names environment \"preview\" but reads no secret" \
	"key-without-environment|build reads TAP_PUSH_KEY under environment nil" \
	"build-whole-context|the jobs reading secrets are [\"build\", \"publish\"]" \
	"build-bracket-key|the jobs reading secrets are [\"build\", \"publish\"]" \
	"build-brace-literal|the jobs reading secrets are [\"build\", \"publish\"]" \
	"publish-action-whole-context|hands a secret to the action actions/github-script@v7"; do
	name="${row%%|*}"
	root="$(tree_for "$name")"
	mutate "$root" "$name"
	if cmp -s .github/workflows/preview.yml "$root/.github/workflows/preview.yml"; then
		printf 'test-preview-contract: mutation %s left the workflow unchanged\n' "$name" >&2
		failures=$((failures + 1))
		continue
	fi
	expect "$name" "$root" "${row#*|}"
done

# --- the other workflows -----------------------------------------------------

root="$(tree_for second-workflow)"
cat >"$root/.github/workflows/tap.yml" <<'YAML'
"on":
  push:
    branches: ["**"]
jobs:
  tap:
    runs-on: ubuntu-24.04
    steps:
      - run: echo "$KEY"
        env:
          KEY: ${{ secrets.TAP_PUSH_KEY }}
YAML
expect second-workflow "$root" 'tap.yml mentions TAP_PUSH_KEY'

# The same reach without the name: a workflow GitHub would admit to the
# environment (dispatched on main), asking for every secret at once.
root="$(tree_for other-workflow-environment)"
cat >"$root/.github/workflows/dispatch.yml" <<'YAML'
"on":
  workflow_dispatch:
jobs:
  leak:
    runs-on: ubuntu-24.04
    environment: preview
    steps:
      - run: echo "$ALL"
        env:
          ALL: ${{ toJSON(secrets) }}
YAML
expect other-workflow-environment "$root" 'dispatch.yml: job leak names environment "preview"'

root="$(tree_for other-workflow-whole-context)"
cat >"$root/.github/workflows/report.yml" <<'YAML'
"on":
  push:
    branches: ["**"]
jobs:
  report:
    runs-on: ubuntu-24.04
    steps:
      - run: echo "$ALL"
        env:
          ALL: ${{ toJSON(secrets) }}
YAML
expect other-workflow-whole-context "$root" 'report.yml: job report reads a secret'

# A reusable-workflow call that passes every secret along names none of them.
root="$(tree_for secrets-inherit)"
cat >"$root/.github/workflows/call.yml" <<'YAML'
"on":
  push:
    branches: ["**"]
jobs:
  call:
    uses: ./.github/workflows/ci.yml
    secrets: inherit
YAML
expect secrets-inherit "$root" 'call.yml: job call passes secrets'

# --- the epoch file ----------------------------------------------------------

root="$(tree_for epoch-absent)"
rm "$root/.github/preview-epoch"
expect epoch-absent "$root" 'there is no .github/preview-epoch'

root="$(tree_for epoch-two-lines)"
printf 'phase4\nphase5\n' >"$root/.github/preview-epoch"
expect epoch-two-lines "$root" '.github/preview-epoch is "phase4\nphase5\n"'

root="$(tree_for epoch-crlf)"
printf 'phase4\r\n' >"$root/.github/preview-epoch"
expect epoch-crlf "$root" '.github/preview-epoch is "phase4\r\n"'

if [ "$failures" -ne 0 ]; then
	printf 'test-preview-contract: %d mutant(s) not caught, %d caught\n' "$failures" "$caught" >&2
	exit 1
fi

printf 'test-preview-contract: ok (%d mutants caught; the unbroken copy passes)\n' "$caught"
