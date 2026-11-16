#!/usr/bin/env bash
# The probe's frames side by side with themselves: a hash of rows 2-40 of each
# snap's text (row 1, the banner, left out), so a page key that paged changes
# the hash and one that did not leaves it. Within each side only: the two
# sides' full transcripts differ before any key (the ledger's causes).
#
# The frames were taken with this receipt's probe-ctrl-pgup-full.cap, from a
# scratch scenarios folder holding it and the four files of fulltranscript.cap
# it names (FXCAP_SCENARIOS=<scratch>/probe-scen):
#   scripts/fxcap/capture.sh fx  probe-ctrl-pgup-full 40x120 <out>   (FX_BIN = fx v0.0.16)
#   scripts/fxcap/capture.sh xfx probe-ctrl-pgup-full 40x120 <out>   (XFX_BIN = built at ce4f9c6b)
#
# usage: frames.sh   (reads this directory)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
for side in fx xfx; do
  for label in full full-ctrl-pgup full-caps-pgup full-pgup-1 full-pgup; do
    f=$here/$side/$side-probe-ctrl-pgup-full-40x120-$label.txt
    printf '%-4s %-15s rows 2-40 sha256 %s\n' "$side" "$label" "$(sed 1d "$f" | shasum -a 256 | cut -c1-12)"
  done
done
