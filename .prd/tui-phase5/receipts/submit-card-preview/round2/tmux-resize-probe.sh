#!/bin/bash
# How tmux (the terminal every capture runs in) moves rows when its window changes size: a pane of 40x120 holding
# `line 01`..`line 60` (so 21..60 on the screen), `composer` on row 38 with the cursor on it and `hint` on row 40,
# shrunk to 24 rows, then grown to 40x100. A tmux server of its own, on a socket in $1, never the user's.
set -u
dir="$1"; sock="$dir/probe.sock"
cat > "$dir/feed.sh" <<'FEED'
#!/bin/bash
stty -echo
for i in $(seq 1 60); do printf 'line %02d\n' "$i"; done
printf '\033[38;1Hcomposer\033[40;1Hhint\033[38;9H'
exec sleep 30
FEED
chmod +x "$dir/feed.sh"
t() { tmux -f /dev/null -S "$sock" "$@"; }
t new-session -d -x 120 -y 40 -s p "$dir/feed.sh"; sleep 1
echo "== 40x120, rows 36-40"; t capture-pane -p -t p | awk '{printf "%2d|%s\n", NR, $0}' | sed -n '36,40p'
t resize-window -t p -y 24; sleep 0.5
echo "== 24x120, rows 1-2 and 19-24"; t capture-pane -p -t p | awk '{printf "%2d|%s\n", NR, $0}' | sed -n '1,2p;19,24p'
t resize-window -t p -y 40 -x 100; sleep 0.5
echo "== 40x100, rows 1-2 and 34-40"; t capture-pane -p -t p | awk '{printf "%2d|%s\n", NR, $0}' | sed -n '1,2p;34,40p'
t kill-server
