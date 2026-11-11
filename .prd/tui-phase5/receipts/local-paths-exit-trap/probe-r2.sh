#!/bin/sh
# probe-r2.sh <bash> <dir>: round 2's probes of bash: (a) an arithmetic error inside a
# function called at the top level; (b) a function named `[`; and, under the guarded EXIT
# trap, (c) SIGINT to the shell and its foreground child at once and (d) a subshell's
# `exit 5` under set -e. One line each, no path.
b=$1 d=$2; mkdir -p "$d"
printf '# bash %s\n' "$("$b" -c 'printf %s "$BASH_VERSION"')"
# (a) an arithmetic error inside a function called at top level
cat > "$d/a.sh" <<'S'
set -euo pipefail
n=0
f() { local i; for i in 1 2 3 4 5; do n=$((n + 1)); if [ "$i" = 3 ]; then : $((1 / 0)); fi; done; echo "f finished"; }
f
echo "after f: n=$n"
g() { echo "g ran"; }; g; echo "end"
S
out=$("$b" "$d/a.sh" 2>"$d/a.err"); echo "a: exit=$? out=[$(echo $out)] err=[$(sed 's#^.*: line#line#' "$d/a.err" | tr '\n' ' ')]"
# (b) `[` as a function name
out=$("$b" -c '[() { echo shadowed; builtin [ "$@"; }; [ 1 = 1 ] && echo yes' 2>&1); echo "b: exit=$? out=[$(echo $out)]"
# (c) INT to the shell and its foreground child at once; (d) a subshell exit 5 under set -e
for k in int-group subshell; do
cat > "$d/$k.sh" <<S
set -euo pipefail
dir="\$(mktemp -d "\${TMPDIR:-/tmp}/p.XXXXXX")"; echo "\$dir" > "\$1"
finished=0
trap 'stopped=\$?; rm -rf "\$dir"; [ "\$finished" = 1 ] || exit "\$((stopped == 0 ? 1 : stopped))"' EXIT
case $k in
int-group) sh -c 'kill -INT "\$1"; kill -INT \$\$' sh "\$\$" ;;
subshell) ( exit 5 ) ;;
esac
echo reached-the-end
finished=1
S
out=$("$b" "$d/$k.sh" "$d/$k.dir" 2>/dev/null); rc=$?; gone=no; [ -e "$(cat "$d/$k.dir")" ] || gone=yes
echo "$k: exit=$rc out=[$out] scratch-removed=$gone"
done
