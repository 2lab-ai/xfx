#!/bin/sh
# Measure every Unicode scalar on tmux and libghostty-vt against xfx's own
# count, and regenerate src/tui/drawn_wider.rs from what they draw wider.
#
# usage: scripts/drawn-wider/sweep.sh <libghostty-vt prefix> <work dir>
#
# <libghostty-vt prefix> is a built libghostty-vt install (its include/ and
# lib/; herdr vendors one). <work dir> takes the intermediate files. tmux runs
# on a private server (`-L xfx-drawn-wider -f /dev/null`), never the default
# one. Needs clang, python3, tmux and a UTF-8 locale named en_US.UTF-8.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
prefix=$1
work=$2
mkdir -p "$work"

# Every scalar but the C0 and C1 controls and the surrogates.
python3 -I -c '
for code in range(0x110000):
    if code < 0x20 or 0x7F <= code <= 0x9F or 0xD800 <= code <= 0xDFFF:
        continue
    print(f"{code:x}")
' > "$work/scalars.txt"

clang -O2 -I"$prefix/include" -L"$prefix/lib" -lghostty-vt -Wl,-rpath,"$prefix/lib" \
    -o "$work/probe_ghostty" "$here/probe_ghostty.c"
"$work/probe_ghostty" < "$work/scalars.txt" > "$work/ghostty.txt"

export LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8
private() { tmux -L xfx-drawn-wider -f /dev/null -u "$@"; }
private kill-server 2>/dev/null || true
rm -f "$work/tmux.txt" "$work/tmux.part"
private new-session -d -s sweep -x 200 -y 10 \
    "python3 -I '$here/dsr_probe.py' '$work/scalars.txt' '$work/tmux.part' && mv '$work/tmux.part' '$work/tmux.txt'"
waited=0
while [ ! -f "$work/tmux.txt" ]; do
    if [ "$waited" -ge 900 ]; then
        private kill-server 2>/dev/null || true
        echo "sweep: tmux did not finish in 900 s" >&2
        exit 1
    fi
    sleep 1
    waited=$((waited + 1))
done
private kill-server 2>/dev/null || true

(cd "$root" && XFX_DRAWN_WIDER_LIST="$work/scalars.txt" XFX_DRAWN_WIDER_OUT="$work/xfx.txt" \
    cargo test --locked --lib -- --ignored --exact tui::drawn_wider::tests::xfx_counts)

python3 -I "$here/generate.py" "$work" "$(date +%Y-%m-%d)" "$(tmux -V)" \
    "libghostty-vt $(cat "$prefix/../VERSION" 2>/dev/null || echo build)" \
    > "$root/src/tui/drawn_wider.rs"
echo "sweep: $(wc -l < "$work/scalars.txt") scalars measured in ${waited} s of tmux"
