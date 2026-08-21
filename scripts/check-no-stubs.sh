#!/usr/bin/env bash
#
# Enforces the no-stub rule from the design:
#
#   1. Production code contains no `todo!`, `unimplemented!`, or placeholder
#      success / canned output.
#   2. Every surface the binary advertises -- command, entrypoint, tool, or
#      shell slash command -- has an `implemented` row in docs/parity.md.
#   3. Every `implemented` row names a surface the binary really advertises.
#      Without this direction "implemented" is an unbacked claim: a row could
#      promise a command that does not exist and nothing would notice.
#   4. No name from a `deferred` row is advertised, including the names listed
#      inside a grouped row such as "file management (`delete_file`, ...)".
#      Those were previously invisible to this check, because a grouped row does
#      not start with a backticked name.
#   5. No surface name appears in more than one row, so "exactly one row per
#      surface" is a property rather than an intention.
#
# The check is text-level on purpose: it must run without building, so a broken
# build cannot hide a broken promise. `tests/parity.rs` runs the same
# reconciliation against the built binary's real parser and tool schemas.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

parity="docs/parity.md"
cli_source="src/cli.rs"
tools_source="src/tools/mod.rs"
shell_source="src/interactive.rs"

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

# Every backticked identifier named by a row of one kind and status, including
# the ones inside a grouped row's prose. A row like
#
#   | file management (`delete_file`, `rename_file`) | tool group | deferred | ...
#
# names two surfaces and starts with neither of them, so `parity_names` above
# cannot see either. Those are exactly the names a schema must not contain.
parity_mentioned_names() {
	awk -v kind="$1" -v status="$2" -F' *\\| *' '
		NF < 6 { next }
		$3 != kind || $4 != status { next }
		{
			surface = $2
			while (match(surface, /`[^`]+`/)) {
				print substr(surface, RSTART + 1, RLENGTH - 2)
				surface = substr(surface, RSTART + RLENGTH)
			}
		}
	' "$parity"
}

# The declared status of one exact surface name, empty when it has no row.
parity_status_of() {
	awk -v kind="$2" -v name="$3" -F' *\\| *' '
		$2 == "`" name "`" && $3 == kind { print $4; exit }
	' "$1"
}

# Reconciles one advertised inventory against the ledger, both ways.
#
# `advertised` is the newline-separated list of names the binary really offers;
# `kind` is the parity row kind that describes them; `group_kind`, when given,
# is the kind whose grouped rows also name surfaces of this sort (tools have
# `tool group` rows, slash commands do not).
check_inventory() {
	local source="$1" advertised="$2" kind="$3" group_kind="${4:-}"
	local name status

	# 1. everything advertised is documented as implemented
	while IFS= read -r name; do
		[ -n "$name" ] || continue
		status="$(parity_status_of "$parity" "$kind" "$name")"
		if [ -z "$status" ]; then
			fail "$kind \`$name\` is advertised by $source but has no row in $parity"
		elif [ "$status" != "implemented" ]; then
			fail "$kind \`$name\` is advertised by $source but its parity row says '$status'"
		fi
	done <<<"$advertised"

	# 2. everything documented as implemented is really advertised
	while IFS= read -r name; do
		[ -n "$name" ] || continue
		if ! printf '%s\n' "$advertised" | grep -qxF -- "$name"; then
			fail "$kind \`$name\` is documented as implemented but $source does not advertise it"
		fi
	done <<<"$(parity_names "$kind" implemented)"

	# 3. nothing documented as deferred is advertised, grouped rows included
	while IFS= read -r name; do
		[ -n "$name" ] || continue
		if printf '%s\n' "$advertised" | grep -qxF -- "$name"; then
			fail "$kind \`$name\` is documented as deferred but $source advertises it"
		fi
	done <<<"$(
		parity_mentioned_names "$kind" deferred
		[ -n "$group_kind" ] && parity_mentioned_names "$group_kind" deferred
	)"
}

# The command surface is two declarations: the subcommands clap parses, and the
# entrypoints that have no name to type. Both are commands to a user, so both
# are reconciled against the `command` rows as one inventory.
if ! grep -q 'pub const ADVERTISED_COMMANDS: &\[&str\] = &\[' "$cli_source"; then
	fail "$cli_source no longer declares ADVERTISED_COMMANDS; the command inventory is unverifiable"
elif ! grep -q 'pub const ADVERTISED_ENTRYPOINTS: &\[&str\] = &\[' "$cli_source"; then
	fail "$cli_source no longer declares ADVERTISED_ENTRYPOINTS; the command inventory is unverifiable"
else
	check_inventory "$cli_source" "$(
		inventory_from "$cli_source" ADVERTISED_COMMANDS
		inventory_from "$cli_source" ADVERTISED_ENTRYPOINTS
	)" command
fi

# The tool registry declares its inventory in the same reconcilable form as the
# command grammar: a flat `&[&str]` this script can read without building.
if [ -d src/tools ]; then
	if [ ! -f "$tools_source" ] || ! grep -q 'pub const ADVERTISED_TOOLS: &\[&str\] = &\[' "$tools_source"; then
		fail "src/tools exists but $tools_source does not declare ADVERTISED_TOOLS"
	else
		check_inventory "$tools_source" \
			"$(inventory_from "$tools_source" ADVERTISED_TOOLS)" tool "tool group"
	fi
fi

# The shell's slash commands are a third advertised surface with the same
# promise: a name `/help` prints is a name the shell answers.
if [ -f "$shell_source" ]; then
	if ! grep -q 'pub const SLASH_COMMANDS: &\[&str\] = &\[' "$shell_source"; then
		fail "$shell_source exists but does not declare SLASH_COMMANDS"
	else
		check_inventory "$shell_source" \
			"$(inventory_from "$shell_source" SLASH_COMMANDS)" slash
	fi
fi

# --- 2b. one row per surface -------------------------------------------------

while IFS= read -r name; do
	[ -n "$name" ] || continue
	fail "surface \`$name\` has more than one row in $parity"
done <<<"$(awk -F' *\\| *' '
	NF < 6 { next }
	$2 !~ /^`.*`$/ { next }
	{
		name = $2
		gsub(/`/, "", name)
		seen[name]++
	}
	END { for (name in seen) if (seen[name] > 1) print name }
' "$parity" | sort)"

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
		if ($3 !~ /^(command|slash|tool|tool group|provider|persistence|ui|embedding)$/) {
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
