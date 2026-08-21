#!/usr/bin/env bash
#
# Refuses to let a credential be committed.
#
# fxr's whole security story is that it reads a token from the environment,
# sends it to exactly one endpoint, and never writes it anywhere -- not into a
# session, not into a snapshot, not into a log. A token checked into the
# repository would make that story false before the product even runs, and the
# most common way one arrives is a debugging session that was never cleaned up.
#
# The scan is over *tracked* files: what a push would publish. It looks for the
# shapes real credentials have -- known issuer prefixes, PEM private keys, JWTs,
# and long bearer literals -- rather than for the names of environment
# variables, which this repository is supposed to talk about.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0

fail() {
	printf 'check-no-secrets: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# The files a push would publish. Outside a checkout, fall back to the working
# tree minus build output, so the check still runs in an unpacked tarball.
#
# Read into an array with a loop rather than with `mapfile`, which is bash 4 and
# therefore absent from the bash macOS ships.
files=()
while IFS= read -r file; do
	[ -n "$file" ] || continue
	files+=("$file")
done < <(
	if git rev-parse --git-dir >/dev/null 2>&1; then
		git ls-files
	else
		find . -type f -not -path './target/*' -not -path './.git/*' | sed 's|^\./||'
	fi
)

if [ "${#files[@]}" -eq 0 ]; then
	fail "no files to scan; the check would pass for the wrong reason"
	exit 1
fi

# `name:regex` pairs. Each pattern describes a credential's *shape*: matching one
# means a real secret, not a mention of one.
patterns=(
	"OpenAI-style key:sk-[A-Za-z0-9]{20,}"
	"GitHub token:gh[pousr]_[A-Za-z0-9]{20,}"
	"GitHub fine-grained token:github_pat_[A-Za-z0-9_]{20,}"
	"AWS access key id:AKIA[0-9A-Z]{16}"
	"Google API key:AIza[0-9A-Za-z_-]{30,}"
	"Slack token:xox[abprs]-[0-9A-Za-z-]{10,}"
	"private key block:-----BEGIN [A-Z ]*PRIVATE KEY-----"
	"JSON web token:eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\."
	"bearer literal:[Bb]earer [A-Za-z0-9_.=-]{24,}"
)

for entry in "${patterns[@]}"; do
	name="${entry%%:*}"
	pattern="${entry#*:}"
	hits="$(grep -n -E -e "$pattern" "${files[@]}" 2>/dev/null || true)"
	if [ -n "$hits" ]; then
		fail "possible $name:"
		printf '%s\n' "$hits" >&2
	fi
done

# A `.env` file is never a source file here: the design keeps credentials in the
# process environment, so one that is tracked is a mistake whatever it contains.
for file in "${files[@]}"; do
	case "$(basename "$file")" in
	.env | .env.*)
		fail "$file is tracked; credential files belong outside the repository"
		;;
	esac
done

if [ "$failures" -ne 0 ]; then
	printf 'check-no-secrets: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-no-secrets: ok (%d tracked file(s) scanned)\n' "${#files[@]}"
