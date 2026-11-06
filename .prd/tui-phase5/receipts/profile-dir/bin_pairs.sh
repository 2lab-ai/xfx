#!/bin/bash
# The binary-level pair: one xfx binary, each writer case, from a fresh HOME.
# usage: bin_pairs.sh <xfx> <label>   (writes under <tmp>/<label>)
set -u
xfx=$1
label=$2
tmp=<tmp>
drive=$(dirname "$0")/drive_tui.py
umask 022
echo "# $label: $(shasum -a 256 "$xfx" | cut -c1-16)  umask $(umask)"
for case in fresh open link; do
	r=$tmp/$label/$case
	rm -rf "$r"
	mkdir -p "$r/HOME" "$r/work" "$r/elsewhere"
	chmod 700 "$r/elsewhere"
	case $case in
	open) mkdir "$r/HOME/.xfx" && chmod 755 "$r/HOME/.xfx" ;;
	link) ln -s "$r/elsewhere" "$r/HOME/.xfx" ;;
	esac
	echo "## xfx setup gateway, ~/.xfx $case"
	(cd "$r/work" && timeout 20 env -i PATH=/usr/bin:/bin HOME="$r/HOME" TMPDIR=$tmp "$xfx" setup gateway > "$r/setup.out" 2>&1; echo "setup exit=$?")
	grep -v '^\[setup\]' "$r/setup.out"
	stat -f '%Sp %N' "$r/HOME/.xfx"
	if [ -d "$r/HOME/.xfx" ] && [ ! -L "$r/HOME/.xfx" ]; then
		echo "entries in ~/.xfx: $(ls -A "$r/HOME/.xfx" | tr '\n' ' ')"
	fi
	echo "entries where a link points: $(ls -A "$r/elsewhere" | tr '\n' ' ')"
	(cd "$r/work" && timeout 20 env -i PATH=/usr/bin:/bin HOME="$r/HOME" TMPDIR=$tmp "$xfx" sessions > "$r/sessions.out" 2>&1; echo "sessions exit=$?")
	head -1 "$r/sessions.out"
done
r=$tmp/$label/tui
rm -rf "$r"
mkdir -p "$r/HOME" "$r/work"
echo "## XFX_TUI=1: /skills create demo, then /skills add <that skill>"
python3 -I "$drive" "$xfx" "$r/HOME" "$r/work" "/skills create demo" "/skills add $r/HOME/.xfx/skills/demo"
find "$r/HOME/.xfx" -type d -exec stat -f '%Sp %N' {} \; | sort -k2
python3 -I -c "import re,sys; t=open(sys.argv[1],'rb').read().decode('utf-8','replace'); t=re.sub(r'\x1b\[[0-9;?<>]*[A-Za-z]|\x1b\][^\x07]*\x07','',t); [print(x.strip()) for x in re.findall(r'\* skills: [^┃]*', t)]" "$r/tui.out"
r=$tmp/$label/tui-gone
rm -rf "$r"
mkdir -p "$r/HOME" "$r/work"
echo "## XFX_TUI=1: ~/.xfx removed under the running session, then /skills create demo"
python3 -I "$drive" "$xfx" "$r/HOME" "$r/work" "!rmprofile" "/skills create demo"
find "$r/HOME/.xfx" -type d -exec stat -f '%Sp %N' {} \; | sort -k2
(cd "$r/work" && timeout 20 env -i PATH=/usr/bin:/bin HOME="$r/HOME" TMPDIR=$tmp "$xfx" sessions > "$r/sessions.out" 2>&1; echo "sessions after exit=$?")
head -1 "$r/sessions.out"
