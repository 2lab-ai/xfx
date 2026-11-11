#!/bin/bash
# usage: suite-lock.sh bench-r3.sh <dir> <tag>   -- <dir> holds the release lib
# test binaries bench-base-3c4b519d (round 2's head) and bench-head-f8daa194
# (round 3's), and tree100k (the AT-RANK receipt's gen_tree.py). One hold of
# the suite lock: on each fixture, the base and then the head, each alone.
S=$1 tag=$2
cd "$S" || exit 2
./bench.sh ./bench-base-3c4b519d "base 3c4b519d (round 2), release lib test binary, $tag" "$S/tree100k"
echo
./bench.sh ./bench-head-f8daa194 "head f8daa194 (round 3), release lib test binary, $tag" "$S/tree100k"
echo
./bench.sh ./bench-base-3c4b519d "base 3c4b519d (round 2), release lib test binary, $tag"
echo
./bench.sh ./bench-head-f8daa194 "head f8daa194 (round 3), release lib test binary, $tag"
