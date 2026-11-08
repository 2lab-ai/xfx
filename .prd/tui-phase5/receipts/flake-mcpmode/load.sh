#!/bin/sh
# usage: load.sh <pidfile> <cores-file> <seconds>
# One `yes > /dev/null` per line of <cores-file>, each bounded by `timeout`;
# this shell's pid (then xargs's, by exec) is written to <pidfile>.
echo $$ > "$1"
exec xargs -P 64 -n 1 sh -c "exec timeout $3 yes > /dev/null" < "$2"
