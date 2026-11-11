#!/usr/bin/env bash
# Train 13 round 2 (opus M1): merge 3's ledger blocks, counted in both of
# git's conflict styles, and blocks.py's own line for each diff3 block. In a
# `git clone --no-local`: bbdde09c's first parent checked out, its unit merged
# with `--no-commit`; the ledger written once in git's default `merge` style
# and once in `diff3` style (`checkout --conflict=<style>`), the `<<<<<<<`
# markers counted and placed, then blocks.py run on the diff3 file, as
# reproduce.sh runs it. Why the counts differ, as read in git's xdiff
# (xdiff/xmerge.c, not measured here beyond the counts): the default `merge`
# style simplifies conflicts at its "zealous" level, which joins two
# conflicts that a few unchanged lines part into one block; for `diff3`
# output, which must show each block's base, git lowers the level to
# "eager", which does not join them. So `amendment-card-after-stop`'s `xfx`
# line and its `why` line, one line apart, are one block in the merge style
# and two in diff3. remerge-3.txt (`git show --remerge-diff`) is written in
# the merge style; blocks.py reads diff3.
# usage: blocks_m3.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
L=scripts/fxcap/parity-ledger.toml
rm -rf "$dir/bm3"
git clone -q --no-local "$repo" "$dir/bm3" || exit 2
cd "$dir/bm3" || exit 2
git config user.email train13@invalid; git config user.name train13
git checkout -q --detach bbdde09c^1
git merge --no-ff --no-commit a37a969e > /dev/null 2>&1
for style in merge diff3; do
  git checkout -q --conflict="$style" "$L"
  n=$(grep -c '^<<<<<<< ' "$L")
  echo "## $L in git's $style style: $n blocks, at lines $(grep -n '^<<<<<<< ' "$L" | cut -d: -f1 | tr '\n' ' ')"
done
echo "## the two styles' blocks around amendment-card-after-stop"
for style in merge diff3; do
  git checkout -q --conflict="$style" "$L"
  start=$(grep -n '^id = "amendment-card-after-stop"' "$L" | cut -d: -f1)
  echo "# $style: $(awk -v s="$start" 'NR > s && NR < s + 30 && /^<<<<<<< /{c++} END{print c+0}' "$L") block(s) within the entry's 30 lines"
done
echo "## blocks.py on the diff3-style file (reproduce.sh's run, its output kept)"
python3 -I -B "$here/blocks.py" "$L"
echo "# blocks.py exit $?"
git merge --abort 2>/dev/null || git reset -q --hard
cd / && rm -rf "$dir/bm3"
