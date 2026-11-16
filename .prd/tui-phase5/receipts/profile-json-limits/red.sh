#!/bin/bash
# RED: the head's tests over the base's src (f74d2fd5, unless a third argument
# names another), on a throwaway clone (`git clone --no-local`, never the
# worktree). `tests/` stay the head's; the two config tests that call only what
# the base has (`RuntimeConfig::load_with`, `read_layer`, `parse_layer`) are
# copied from the head's `src/config.rs` into the base's test module. The
# module `config::layer_json` and its tests have no base to fail on. Each run
# bounded at 900 s; the clone and TMPDIR print as <clone> and <tmp>.
# usage: red.sh <head repo> <clone dir> [src rev]
set -u
head=$1 clone=$2 rev=${3:-f74d2fd5}
case "$clone" in "$head"*) echo "the clone must not be inside the worktree"; exit 2 ;; esac
rm -rf "$clone"
git clone -q --no-local "$head" "$clone" || exit 2
git -C "$clone" checkout -q "$rev" -- src
# The two tests, cut from the head's config.rs from the first one's doc comment
# to the `private` helper both use, go in before that helper in the base's.
python3 - "$head/src/config.rs" "$clone/src/config.rs" <<'EOF' || exit 2
import sys
head, base = (open(p).read() for p in sys.argv[1:3])
start = head.index("    /// fx starts on a number past `f64` and on any nesting at a key nothing")
end = head.index("    /// Sets `path`'s mode bits, whatever the umask made them.")
tests = head[start:end]
anchor = "    /// Sets `path`'s mode bits, whatever the umask made them.\n"
assert base.count(anchor) == 1
open(sys.argv[2], "w").write(base.replace(anchor, tests + anchor))
EOF
echo "clone of $(git -C "$clone" rev-parse --short HEAD), src at $rev, two config tests copied in"
for t in "cli ask_goes_on_with_a_profile_fx_starts_with" \
  "cli ask_refuses_a_profile_fx_refuses" \
  "lib config::tests::a_number_past_f64_or_a_deep_value_is_read_as_fxs_parse_reads_it" \
  "lib config::tests::a_project_file_fx_reads_is_applied"; do
  set -- $t
  if [ "$1" = lib ]; then target=--lib; else target="--test $1"; fi
  out=$(cd "$clone" && perl -e 'alarm 900; exec @ARGV' cargo test --locked -q $target -- --exact "$2" 2>&1)
  code=$?
  why=$(printf '%s\n' "$out" | grep -m1 -A3 'panicked at' | tr '\n' ' ' | cut -c1-400)
  echo "$1::$2 exit=$code ${why:-every test passed}" | sed -e "s#$clone#<clone>#g" -e "s#${TMPDIR:-/nonexistent}#<tmp>#g"
done
