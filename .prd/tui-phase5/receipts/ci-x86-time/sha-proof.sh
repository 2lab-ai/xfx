#!/bin/bash
# usage: sha-proof.sh   (round 2; from the worktree, right after the local
#                        gate, which left target/ as the one-job order leaves it)
#
# The x86_64 macOS row's `release` part builds everything it runs in a fresh
# target/, with none of the `checks` part's debug steps before it. This shows
# each binary a release-part check drives has the bytes the one-job order gave
# it. Mach-O binaries carry a linker UUID that follows the target directory's
# path (round 1), so every comparison is made at target/, the path CI uses.
#
#   1. note every binary the gate built; move target/ to target-gate/
#   2. build a fresh target/ in the release part's order:
#        release --lib tests, default and fault-injection  (output cost)
#        debug --lib tests                                  (usage clock, debug)
#        release build                                      (smoke, smoke-tui)
#        fault-injection build, --target-dir target/faulty  (smoke-tui)
#   3. compare each pair by sha256
#   4. the debug lib tests are never the same bytes twice: an unstripped build
#      names each object file with a per-build token in its debug map. So 2c
#      is held to what two fresh builds of it share -- everything but those
#      tokens and the code signature -- and a control pair (two fresh builds,
#      same command, same path) shows the same difference.
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
# exe <profile-flag-or-empty> [features...] -> the lib test binary's path.
# Refuses a lookup that had to compile: that would mean the binary looked up
# was not the one the gate (or step 2) built.
exe() {
	local release=$1 out
	shift
	out="$(cargo test --locked $release --lib --no-run "$@" 2>&1)"
	if printf '%s\n' "$out" | grep -q '^ *Compiling '; then
		printf 'LOOKUP COMPILED (%s %s): not the binary already built\n' "$release" "$*" >&2
		fails=$((fails + 1))
	fi
	printf '%s\n' "$out" | sed -n 's/^ *Executable unittests src\/lib.rs (\(.*\))$/\1/p'
}
quiet() { "$@" 2>&1 | grep -E '^ *(Compiling|Finished)' | sed -E 's/ in [0-9.]+s$//; s/ \(\/[^)]*\)//'; }

printf 'head %s\n' "$(git rev-parse HEAD)"
printf 'host %s, %s\n\n' "$(rustc -vV | sed -n 's/^host: //p')" "$(rustc --version)"
[ -d target-gate ] && { echo "target-gate exists; refusing to overwrite" >&2; exit 2; }

gate_release="$(exe --release)"
gate_release_fi="$(exe --release --features fault-injection)"
gate_debug="$(exe '')"
mv target target-gate
g() { printf 'target-gate/%s' "${1#target/}"; }

echo '## 2. fresh target/, in the release part order'
quiet cargo test --locked --release --lib --no-run
quiet cargo test --locked --release --lib --no-run --features fault-injection
quiet cargo test --locked --lib --no-run
quiet cargo build --locked --release
quiet cargo build --locked --release --features fault-injection --target-dir target/faulty
fresh_release="$(exe --release)"
fresh_release_fi="$(exe --release --features fault-injection)"
fresh_debug="$(exe '')"
echo
echo '## 3. one-job order (gate) vs release part order (fresh)'
same "2a release lib tests, default (output cost; usage clock's release run)" "$(g "$gate_release")" "$fresh_release"
same "2b release lib tests, fault-injection (output cost; usage clock's run)" "$(g "$gate_release_fi")" "$fresh_release_fi"
objects() { # objects <label> <a> <b>: same, or differing only in object-file tokens
	if cmp -s "$2" "$3"; then
		same "$1" "$2" "$3"
		return
	fi
	printf 'TOKEN %s\n      %s  %s\n      %s  %s\n' "$1" "$(digest "$2")" "$2" "$(digest "$3")" "$3"
	python3 -I "$(dirname "$0")/objname-diff.py" "$2" "$3" || fails=$((fails + 1))
}
objects "2c debug lib tests, default (usage clock's debug run): built by cargo test --all-targets vs by --lib alone" "$(g "$gate_debug")" "$fresh_debug"
same "3  target/release/xfx (smoke, smoke-tui)" target-gate/release/xfx target/release/xfx
same "4  target/faulty/release/xfx (smoke-tui --faulty)" target-gate/faulty/release/xfx target/faulty/release/xfx
echo
echo '## 4. control: the debug lib tests built twice, fresh, the same way'
mv target target-fresh1
quiet cargo test --locked --lib --no-run
objects "4c debug lib tests, default: two fresh builds, same command, same path" "target-fresh1/${fresh_debug#target/}" "$fresh_debug"
echo
printf '%d mismatch(es)\n' "$fails"
[ "$fails" -eq 0 ]
