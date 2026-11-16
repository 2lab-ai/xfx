#!/bin/bash
# RED: the head's tests over the base's src (9c64ddd3, unless a third argument
# names another), on a throwaway clone (`git clone --no-local`, never the
# worktree). PROFILE-REFUSE-TYPED-KEYS's red.sh with this unit's tests. Each
# run bounded at 900 s; the clone and TMPDIR are printed as <clone> and <tmp>.
# The unit tests inside src/ cannot run over the old src; the mutants
# (mutants.py) stand for them.
# usage: red.sh <head repo> <clone dir> [src rev]
set -u
head=$1 clone=$2 rev=${3:-9c64ddd3}
case "$clone" in "$head"*) echo "the clone must not be inside the worktree"; exit 2 ;; esac
rm -rf "$clone"
git clone -q --no-local "$head" "$clone" || exit 2
git -C "$clone" checkout -q "$rev" -- src
echo "clone of $(git -C "$clone" rev-parse --short HEAD), src at $rev"
for t in "cli a_mode_in_any_of_fxs_spellings_is_the_mode_that_runs" \
  "cli full_access_is_said_in_fxs_word_and_recorded_in_the_wire_word" \
  "cli ask_takes_fxs_two_spellings_of_full_access_exactly" \
  "gateway ask_help_advertises_only_the_implemented_flags" \
  "cli status_applies_model_and_permission_mode_overrides" \
  "cli an_invalid_permission_mode_override_is_a_diagnostic_not_a_crash"; do
  set -- $t
  out=$(cd "$clone" && perl -e 'alarm 900; exec @ARGV' cargo test --locked -q --test "$1" -- "$2" 2>&1)
  code=$?
  why=$(printf '%s\n' "$out" | grep -m1 -A3 'panicked at' | tr '\n' ' ' | cut -c1-400)
  echo "$1::$2 exit=$code ${why:-every test passed}" | sed -e "s#$clone#<clone>#g" -e "s#${TMPDIR:-/nonexistent}#<tmp>#g"
done
