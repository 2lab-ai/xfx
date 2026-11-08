#!/bin/bash
# usage: sha-proof.sh   (from the worktree, right after the local gate, which
#                        left target/ as the one-job CI sequence leaves it)
#
# Shows each check drives the bytes it drove before the change. Mach-O
# binaries carry a linker UUID derived from the link's inputs, including the
# object files' paths, so two builds compare byte for byte only at the same
# target-directory path. Every comparison but (5) is therefore made at
# target/, the path CI uses; (5) cannot be, since the old fault-injection build
# used a directory of its own, and its difference is located instead.
#
#   1. the gate's target/ is moved to target-gate/; a fresh target/ is built
#      in the order the `release` part runs, with no debug step before it
#   2. release lib tests (output cost, usage clock), default and
#      fault-injection: after every debug step (one job) vs with none (part)
#   3. release binary: built right after a fault-injection build (new) vs
#      with none before it (old order), both at target/
#   4. fault-injection binary, new way, rebuilt in the fresh target/: the same
#      bytes as the gate's copy (the copy is the build output, deterministic)
#   5. fault-injection binary: own --target-dir (old) vs shared dir (new)
#   6. re-run: a fresh release unit is re-linked to target/release/xfx
set -u
unset GREP_OPTIONS
fails=0
digest() { shasum -a 256 "$1" | cut -d' ' -f1; }
same() { # same <label> <a> <b>
	local a b
	a="$(digest "$2")"
	b="$(digest "$3")"
	if [ "$a" = "$b" ]; then
		printf 'SAME  %s\n      %s  %s\n      %s  %s\n' "$1" "$a" "$2" "$b" "$3"
	else
		printf 'DIFF  %s\n      %s  %s\n      %s  %s\n' "$1" "$a" "$2" "$b" "$3"
		fails=$((fails + 1))
	fi
}
exe() { # exe <target-dir> [features...] -> path of the release lib test binary
	local dir=$1
	shift
	cargo test --locked --release --lib --no-run --target-dir "$dir" "$@" 2>&1 |
		sed -n 's/^ *Executable unittests src\/lib.rs (\(.*\))$/\1/p'
}
quiet() { "$@" 2>&1 | grep -E '^ *(Compiling|Finished)' | sed -E 's/ in [0-9.]+s$//; s/ \(\/[^)]*\)//'; }

head="$(git rev-parse HEAD)"
printf 'head %s\n\n' "$head"
[ -d target-gate ] && { echo "target-gate exists; refusing to overwrite" >&2; exit 2; }

gate_default="$(exe target)"
gate_faulty="$(exe target --features fault-injection)"
mv target target-gate
gate_default="target-gate/${gate_default#target/}"
gate_faulty="target-gate/${gate_faulty#target/}"

echo '## 1. fresh target/, the release part order'
quiet cargo test --locked --release --lib --no-run
quiet cargo test --locked --release --lib --no-run --features fault-injection
fresh_default="$(exe target)"
fresh_faulty="$(exe target --features fault-injection)"
quiet cargo build --locked --release
cp target/release/xfx target/release-built-first
echo
same "2a release lib tests, default: after the debug steps vs none" "$gate_default" "$fresh_default"
same "2b release lib tests, fault-injection: after the debug steps vs none" "$gate_faulty" "$fresh_faulty"
same "3 release binary: after a fault-injection build (gate) vs none before it" target-gate/release/xfx target/release-built-first
echo
echo '## 4. new-way fault-injection build in the fresh target/'
quiet cargo build --locked --release --features fault-injection
same "4 fault-injection binary: gate copy vs rebuilt in shared target/" target-gate/faulty/xfx target/release/xfx
echo
echo '## 5. old-way fault-injection build, --target-dir target/faulty'
quiet cargo build --locked --release --features fault-injection --target-dir target/faulty
old=target/faulty/release/xfx
new=target-gate/faulty/xfx
if cmp -s "$old" "$new"; then
	printf 'SAME  5 fault-injection binary, old vs new\n'
else
	printf 'DIFF  5 fault-injection binary, old (%s) vs new (%s): sizes %s / %s\n' "$old" "$new" \
		"$(wc -c <"$old" | tr -d ' ')" "$(wc -c <"$new" | tr -d ' ')"
	diffs="${TMPDIR:-/tmp}/sha-proof-diff.$$"
	cmp -l "$old" "$new" | awk '{print $1}' >"$diffs"
	python3 -I "$(dirname "$0")/locate-diff.py" "$new" "$diffs" || fails=$((fails + 1))
	rm -f "$diffs"
	otool -l "$old" | awk '/cmd LC_UUID/{u=1} u&&/uuid/{print "      LC_UUID old " $2; u=0}'
	otool -l "$new" | awk '/cmd LC_UUID/{u=1} u&&/uuid/{print "      LC_UUID new " $2; u=0}'
fi
echo
echo '## 6. re-run: release build again, every unit fresh'
quiet cargo build --locked --release
same "6 target/release/xfx is the release binary again" target/release/xfx target/release-built-first
echo
printf '%d mismatch(es) among the SAME checks\n' "$fails"
[ "$fails" -eq 0 ]
