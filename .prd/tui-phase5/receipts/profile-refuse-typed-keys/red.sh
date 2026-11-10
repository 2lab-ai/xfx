#!/bin/bash
# RED: the head's tests over the base's src (0a334851), on a throwaway clone
# (`git clone --no-local`, never the worktree). Each run bounded at 900 s;
# the clone and TMPDIR are printed as <clone> and <tmp>.
# usage: red.sh <head repo> <clone dir>
set -u
head=$1 clone=$2
case "$clone" in "$head"*) echo "the clone must not be inside the worktree"; exit 2 ;; esac
rm -rf "$clone"
git clone -q --no-local "$head" "$clone" || exit 2
git -C "$clone" checkout -q 0a334851 -- src
echo "clone of $(git -C "$clone" rev-parse --short HEAD), src at 0a334851"
for t in "cli ask_refuses_a_profile_fx_refuses" "cli status_stops_where_fxs_load_fails" \
  "interactive the_shell_refuses_a_profile_fx_refuses" "tui a_profile_fx_refuses_is_refused" \
  "cli ask_goes_on_with_a_profile_fx_starts_with" \
  "gateway ask_with_a_permission_table_that_does_not_read_runs_no_tool"; do
  set -- $t
  out=$(cd "$clone" && perl -e 'alarm 900; exec @ARGV' cargo test --locked -q --test "$1" -- "$2" 2>&1)
  code=$?
  why=$(printf '%s\n' "$out" | grep -m1 -A3 'panicked at' | tr '\n' ' ' | cut -c1-400)
  echo "$1::$2 exit=$code ${why:-every test passed}" | sed -e "s#$clone#<clone>#g" -e "s#${TMPDIR:-/nonexistent}#<tmp>#g"
done
