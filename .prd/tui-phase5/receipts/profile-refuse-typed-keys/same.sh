#!/bin/bash
# Two xfx probes, entry for entry, over the fixtures the older holds: every
# line but the build's revision, the terminal's byte count, and what the line
# shell has written on stderr when the probe closes its still-running pane
# after 6 s (the start of a write, `cannot write output`, or nothing yet).
# Prints the lines that differ, then their count.
# usage: same.sh <older probe> <newer probe>
set -u
norm() { # probe [fixture list]: its fixture blocks, those in the list only
  sed -E -e '/^== probe\.sh /d' -e '/^binary sha256/d' -e 's/build_revision=[0-9a-f]+/build_revision=R/g' \
    -e 's/tty=[0-9]+B/tty=NB/' \
    -e '/^  line shell +still running/s/(stderr=|\|)(xfx: (cannot write output: Input\/output error \(os error 5\)\|)?)? \| touched/\1<closed> | touched/' "$1" \
    | awk -v keep="${2:-}" 'BEGIN {n = split(keep, k, " "); for (i = 1; i <= n; i++) want["== " k[i]] = 1}
        /^== / {on = (n == 0 || ($0 in want))} on {print}'
}
names=$(grep '^== ' "$1" | grep -v '^== probe\.sh ' | cut -c4- | tr '\n' ' ')
out=$(diff <(norm "$1") <(norm "$2" "$names"))
printf '%s\n' "$out" | grep '^[<>]'
echo "== $(printf '%s\n' "$out" | grep -c '^[<>]') line(s) differ over $(echo $names | wc -w | tr -d ' ') fixtures"
