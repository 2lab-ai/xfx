#!/bin/bash
# tmux's socket path limit on this machine: a server of its own on a socket
# path of 103 bytes, then 104, under <dir>; each stopped through its socket.
# usage: limit-probe.sh <dir>   (prints lengths and tmux's answer, no path)
set -u
base=$1/probe
mkdir -p "$base"
echo "tmux $(tmux -V)"
for n in 103 104; do
  pad=$(( n - ${#base} - 3 ))
  dir=$base/$(printf 'p%.0s' $(seq 1 $pad))
  mkdir -p "$dir"
  sock=$dir/s
  len=$(printf %s "$sock" | wc -c | tr -d ' ')
  if err=$(tmux -f /dev/null -S "$sock" new-session -d -s probe 'sleep 30' 2>&1); then
    echo "socket path $len bytes: started; $(tmux -f /dev/null -S "$sock" list-sessions -F '#{session_name}' 2>&1)"
    tmux -f /dev/null -S "$sock" kill-server
  else
    echo "socket path $len bytes: refused: $(printf %s "$err" | sed -E 's#/[^ ]+#<path>#g')"
  fi
done
rm -rf "$base"
