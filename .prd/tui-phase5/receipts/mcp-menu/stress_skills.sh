#!/bin/bash
# The skills test alone, N times, one process after another, in the
# throwaway clone at <rev>, its assertion message naming the skills' paths
# (test-only text). Counts failures and prints each failing run's paths.
# usage: stress.sh <clone> <rev> <runs> <scratch> [--features fault-injection]
set -u
M=$1 REV=$2 RUNS=$3 S=$4
shift 4
cd "$M" || exit 1
git checkout -q -- . && git checkout -q -f "$REV" || exit 1
python3 - "$M/src/tui/shell.rs" <<'EOF'
import sys
p = sys.argv[1]
s = open(p).read()
old = '''            "a word no skill matches showed the menu"
        );'''
new = '''            "a word no skill matches showed the menu: {:?}",
            shell.skills.skills().iter().map(|skill| skill.path.display().to_string()).collect::<Vec<_>>()
        );'''
if s.count(old) == 1:
    open(p, "w").write(s.replace(old, new))
EOF
mkdir -p /private/tmp/claude-501/fi
export TMPDIR=/private/tmp/claude-501/fi
bin=$(cargo test --locked "$@" --lib --no-run --message-format=json 2>/dev/null | python3 -c '
import sys, json
for line in sys.stdin:
    try: m = json.loads(line)
    except ValueError: continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["kind"] == ["lib"]:
        print(m["executable"])
' | tail -1)
[ -x "$bin" ] || { echo "no test binary"; exit 1; }
name=tui::shell::tests::a_space_ends_a_dollar_search_and_a_word_nothing_matches_shows_no_menu
fails=0
for i in $(seq 1 "$RUNS"); do
  out=$("$bin" --exact "$name" --test-threads 1 2>&1)
  if ! printf '%s' "$out" | grep -q 'test result: ok. 1 passed'; then
    fails=$((fails + 1))
    echo "run $i FAILED: $(printf '%s' "$out" | grep -o 'showed the menu[^\n]*' | head -1 | cut -c1-400)"
  fi
done
echo "TOTAL: $fails of $RUNS failed"
git checkout -q -- .
