#!/bin/bash
# Both loaded loops in one exclusive hold: the base's binaries, then the head's.
set -u
bash <scratch>/loops.sh "base 2e724248 (58c92ba2 + RED test)" <tmp>/t-base-fi/debug/deps/tui-a9816d5a9344a131 <tmp>/t-base-plain/debug/deps/tui-9905dcb580b5dadb 20 > <scratch>/loops-base.txt 2>&1
echo "exit $?" >> <scratch>/loops-base.txt
bash <scratch>/loops.sh "head ed0e4e3a (the fix and its tests)" <tmp>/t-head-fi/debug/deps/tui-a9816d5a9344a131 <tmp>/t-head-plain/debug/deps/tui-9905dcb580b5dadb 20 > <scratch>/loops-head.txt 2>&1
echo "exit $?" >> <scratch>/loops-head.txt
