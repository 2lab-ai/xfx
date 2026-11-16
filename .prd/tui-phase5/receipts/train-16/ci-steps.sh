#!/usr/bin/env bash
# Train 16 (new): the step list gates.sh is held to. For ci.yml's gate job
# and sweep.yml, at the train's base (train 15's head) and at <head>, each
# step as `name | if | run` (a multi-line `run: |` as its lines joined by
# `; `), then the two lists' difference; then gates.sh's own steps, in its
# order. gates.sh runs every `run` of the gate job but the checkout, the
# toolchain and the runner checks, and the upload-on-failure steps; the
# fault injector's step runs there as a whole `cargo test --features
# fault-injection` (its `--test tui` and `--lib` both inside it), and its
# clippy run as its own step.
# usage: ci-steps.sh <repo> <base> <head>
set -u
repo=$1 base=$2 head=$3
here=$(cd "$(dirname "$0")" && pwd)
steps() { # <commit> <workflow>
  git -C "$repo" show "$1:.github/workflows/$2" | python3 -I -B "$here/ci_steps.py"
}
for wf in ci.yml sweep.yml; do
  echo "## $wf at $base"
  steps "$base" "$wf" > "${TMPDIR:-/tmp}/ci-steps-a.$$"
  cat "${TMPDIR:-/tmp}/ci-steps-a.$$"
  echo "## $wf at $(git -C "$repo" rev-parse --short=8 "$head")"
  steps "$head" "$wf" > "${TMPDIR:-/tmp}/ci-steps-b.$$"
  cat "${TMPDIR:-/tmp}/ci-steps-b.$$"
  echo "## $wf: the steps that differ (< base, > head)"
  diff "${TMPDIR:-/tmp}/ci-steps-a.$$" "${TMPDIR:-/tmp}/ci-steps-b.$$" | grep '^[<>]'
  echo "# $(diff "${TMPDIR:-/tmp}/ci-steps-a.$$" "${TMPDIR:-/tmp}/ci-steps-b.$$" | grep -c '^[<>]') lines differ"
  rm -f "${TMPDIR:-/tmp}/ci-steps-a.$$" "${TMPDIR:-/tmp}/ci-steps-b.$$"
  echo
done
echo "## gates.sh's steps, in its order"
grep -E '^step ' "$here/gates.sh" | nl -w2 -s' '
echo "# $(grep -cE '^step ' "$here/gates.sh") steps"
