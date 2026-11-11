#!/bin/bash
# usage: r4-rowdiff.sh <repo> <work dir> <rev>...
# For each revision: a `git clone --no-local` throwaway at it, the row dump
# (r4-rowdump.py) added, built and run there; its rows in
# <work dir>/rows-<rev>.txt. Then each revision's rows against the first's:
# line counts, prefixes, sha256, and `cmp`.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo=$1 work=$2
shift 2
mkdir -p "$work"
for rev in "$@"; do
	c="$work/clone-$rev"
	rm -rf "$c"
	git clone -q --no-local "$repo" "$c"
	git -C "$c" checkout -q "$rev"
	python3 -I "$here/r4-rowdump.py" "$c/src/tui/file_index.rs"
	(cd "$c" && ROWDIFF_OUT="$work/rows-$rev.txt" cargo test --locked --lib -q -- \
		--exact --ignored tui::file_index::tests::zz_rows_dump >/dev/null 2>&1)
	rm -rf "$c"
done
first=$1
for rev in "$@"; do
	f="$work/rows-$rev.txt"
	printf '%s: %s lines, %s prefixes, %s with no row, sha256 %s\n' "$rev" \
		"$(wc -l <"$f" | tr -d ' ')" \
		"$(cut -f1 "$f" | uniq | wc -l | tr -d ' ')" \
		"$(grep -c $'\tno rows$' "$f" || true)" \
		"$(shasum -a 256 "$f" | cut -c1-16)"
	if [ "$rev" != "$first" ]; then
		if cmp -s "$work/rows-$first.txt" "$f"; then
			echo "$rev against $first: identical"
		else
			echo "$rev against $first: $(diff "$work/rows-$first.txt" "$f" | grep -c '^[<>]') differing lines"
		fi
	fi
done
