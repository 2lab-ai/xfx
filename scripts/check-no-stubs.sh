#!/usr/bin/env bash
#
# Enforces the no-stub rule from the design:
#
#   1. Production code contains no `todo!`, `unimplemented!`, or placeholder
#      success / canned output.
#   2. Every command the parser advertises has an `implemented` row in
#      docs/parity.md.
#   3. Every command documented as `deferred` is absent from the parser.
#   4. Once a tool registry exists, the same reconciliation applies to tools.
#
# The check is text-level on purpose: it must run without building, so a broken
# build cannot hide a broken promise.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

parity="docs/parity.md"
cli_source="src/cli.rs"
tools_source="src/tools/mod.rs"

failures=0

fail() {
	printf 'check-no-stubs: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# --- 1. production stubs -----------------------------------------------------

# Only non-test code is production. A top-level `#[cfg(test)]` module is skipped
# by line range rather than by file, so a test helper cannot be used to smuggle a
# stub into a shipped code path while the rest of the file still gets scanned.
production_lines() {
	awk '
		/^#\[cfg\(test\)\]/ { in_test = 1; next }
		in_test && /^\}[[:space:]]*$/ { in_test = 0; next }
		in_test { next }
		{ print FILENAME ":" FNR ":" $0 }
	' "$1"
}

# Emits `file:line:<literal>` for every string literal on a production line.
production_literals() {
	awk '
		/^#\[cfg\(test\)\]/ { in_test = 1; next }
		in_test && /^\}[[:space:]]*$/ { in_test = 0; next }
		in_test { next }
		{
			line = $0
			while (match(line, /"[^"]*"/)) {
				print FILENAME ":" FNR ":" substr(line, RSTART + 1, RLENGTH - 2)
				line = substr(line, RSTART + RLENGTH)
			}
		}
	' "$1"
}

# Markers that are never legitimate in shipped code, wherever they appear.
unfinished_markers=(
	'todo!'
	'unimplemented!'
	'TODO'
	'FIXME'
	'XXX'
)

# Prose that indicates canned or placeholder output. Checked only inside string
# literals, because a doc comment is allowed to *discuss* the rule it documents
# while a printed string is the thing the rule forbids.
canned_output_markers=(
	'placeholder'
	'not implemented'
	'not yet implemented'
	'coming soon'
	'stub'
)

while IFS= read -r source_file; do
	lines="$(production_lines "$source_file")"
	literals="$(production_literals "$source_file")"

	for pattern in "${unfinished_markers[@]}"; do
		hits="$(printf '%s\n' "$lines" | grep -F -- "$pattern" || true)"
		if [ -n "$hits" ]; then
			fail "unfinished-work marker '$pattern' in production code:"
			printf '%s\n' "$hits" >&2
		fi
	done

	for pattern in "${canned_output_markers[@]}"; do
		hits="$(printf '%s\n' "$literals" | grep -iF -- "$pattern" || true)"
		if [ -n "$hits" ]; then
			fail "canned-output marker '$pattern' in a production string literal:"
			printf '%s\n' "$hits" >&2
		fi
	done
done < <(find src build.rs -name '*.rs' -type f 2>/dev/null | sort)

# --- 2. inventory reconciliation --------------------------------------------

if [ ! -f "$parity" ]; then
	fail "$parity is missing; the parity ledger is not optional"
	printf 'check-no-stubs: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

# Reads the names inside `pub const <NAME>: &[&str] = &[ ... ];`, whether the
# declaration is on one line or many.
inventory_from() {
	awk -v const="$2" '
		index($0, "pub const " const ": &[&str] = &[") { capturing = 1 }
		capturing {
			line = $0
			if (match(line, /\];/)) { line = substr(line, 1, RSTART); capturing = 0 }
			while (match(line, /"[^"]*"/)) {
				print substr(line, RSTART + 1, RLENGTH - 2)
				line = substr(line, RSTART + RLENGTH)
			}
			if (!capturing) exit
		}
	' "$1"
}

# All surface names of one kind and status, one per line.
parity_names() {
	grep -E "^\| \`[^\`]+\` \| $1 \| $2 \|" "$parity" 2>/dev/null |
		sed -E 's/^\| `([^`]+)`.*/\1/' || true
}

# The declared status of one exact surface name, empty when it has no row.
parity_status_of() {
	awk -v kind="$2" -v name="$3" -F' *\\| *' '
		$2 == "`" name "`" && $3 == kind { print $4; exit }
	' "$1"
}

check_inventory() {
	local file="$1" const="$2" kind="$3"
	local advertised name status
	advertised="$(inventory_from "$file" "$const")"

	while IFS= read -r name; do
		[ -n "$name" ] || continue
		status="$(parity_status_of "$parity" "$kind" "$name")"
		if [ -z "$status" ]; then
			fail "$kind \`$name\` is advertised by $file but has no row in $parity"
		elif [ "$status" != "implemented" ]; then
			fail "$kind \`$name\` is advertised by $file but its parity row says '$status'"
		fi
	done <<<"$advertised"

	while IFS= read -r name; do
		[ -n "$name" ] || continue
		if printf '%s\n' "$advertised" | grep -qxF -- "$name"; then
			fail "$kind \`$name\` is documented as deferred but $file advertises it"
		fi
	done <<<"$(parity_names "$kind" deferred)"
}

if ! grep -q 'pub const ADVERTISED_COMMANDS: &\[&str\] = &\[' "$cli_source"; then
	fail "$cli_source no longer declares ADVERTISED_COMMANDS; the command inventory is unverifiable"
else
	check_inventory "$cli_source" ADVERTISED_COMMANDS command
fi

# The tool registry declares its inventory in the same reconcilable form as the
# command grammar: a flat `&[&str]` this script can read without building.
if [ -d src/tools ]; then
	if [ ! -f "$tools_source" ] || ! grep -q 'pub const ADVERTISED_TOOLS: &\[&str\] = &\[' "$tools_source"; then
		fail "src/tools exists but $tools_source does not declare ADVERTISED_TOOLS"
	else
		check_inventory "$tools_source" ADVERTISED_TOOLS tool
	fi
fi

# --- 3. every parity row carries a recognized kind and status ----------------

# An inventory row is `| `name` | kind | status | notes |`; the legend table at
# the top of the file has only two columns and is skipped by field count. An
# unrecognized kind is a failure too, because a typo there would make the row
# invisible to the reconciliation above.
while IFS= read -r problem; do
	[ -n "$problem" ] || continue
	fail "$problem"
done <<<"$(awk -F' *\\| *' '
	NF < 6 { next }
	$2 !~ /^`.*`$/ { next }
	{
		name = $2
		gsub(/`/, "", name)
		if ($3 !~ /^(command|tool|tool group|provider|persistence|ui|embedding)$/) {
			print "parity row `" name "` has an unrecognized kind \"" $3 "\""
		}
		if ($4 !~ /^(implemented|partial|deferred)$/) {
			print "parity row `" name "` has an unrecognized status \"" $4 "\""
		}
	}
' "$parity")"

if [ "$failures" -ne 0 ]; then
	printf 'check-no-stubs: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-no-stubs: ok\n'
