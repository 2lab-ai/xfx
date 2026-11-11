#!/bin/bash
# usage: suite-lock.sh bench-all.sh <dir> <tag>   -- <dir> holds the two release
# lib test binaries (bench-base-4a5dc3c7, bench-head-b47953e2) and tree100k
# (the AT-RANK receipt's gen_tree.py). One hold of the suite lock: base and head on the 100k tree, then the head
# on fx's inventory, one after another, each alone.
S=$1 tag=$2
cd "$S" || exit 2
./bench.sh ./bench-base-4a5dc3c7 "base 4a5dc3c7 (AT-RANK), release lib test binary, $tag" "$S/tree100k"
echo
./bench.sh ./bench-head-b47953e2 "head b47953e2 (AT-RANK-PERF), release lib test binary, $tag" "$S/tree100k"
echo
./bench.sh ./bench-head-b47953e2 "head b47953e2 (AT-RANK-PERF), release lib test binary, $tag"
