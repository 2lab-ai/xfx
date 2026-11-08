#!/usr/bin/env bash
# Train 9, join check 5's test on each side alone: PASTE-AFTER-END's tip
# (a832f4d7, cap 4) and QUESTIONS-20's (3824f6ba, X1's refusal). On a `git
# clone --no-local` of each: the test appended unchanged
# (apply_join_paste.py); where it does not build, QUESTIONS-20's test
# helpers it names (`question_batch`, `tool_output`) copied in from the head
# (test code only); then run. Train 8's join_units.sh, for one test.
# usage: join_units_paste.sh <repo> <scratch>   (inside one hold of the suite lock)
set -u
repo=$1 scratch=$2
here=$(cd "$(dirname "$0")" && pwd)
TEST=a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing
for side in paste-after-end=a832f4d7 questions-20=3824f6ba; do
  name=${side%%=*} rev=${side#*=}
  clone=$scratch/alone-paste-$name
  rm -rf "$clone"
  git clone -q --no-local "$repo" "$clone" && git -C "$clone" checkout -q "$rev"
  python3 -I -B "$here/apply_join_paste.py" "$clone"
  ( cd "$clone" && cargo test --locked --test tui --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  errors=$(grep -c '^error\[' "$clone.build.log")
  echo "## $name $(git -C "$clone" rev-parse --short=8 HEAD), the test unchanged: build exit $rc, $errors errors"
  grep -A4 '^error\[' "$clone.build.log" | grep -E '^(error|  *-->)' | sed 's/^/    /' | head -12
  python3 -I -B - "$here" "$clone" <<'PY'
import subprocess, sys
here, clone = sys.argv[1:3]
def item(text, start):
    i = text.index(start)
    doc = text.rfind("\n\n", 0, i) + 2
    end = text.index("\n}\n", i) + 3
    return text[doc:end]
path = "tests/tui.rs"
mine = open(f"{clone}/{path}").read()
missing = [s for s in ("fn question_batch(", "fn tool_output(") if s not in mine]
if missing:
    there = subprocess.run(["git", "-C", here, "show", f"HEAD:{path}"], check=True, capture_output=True, text=True).stdout
    open(f"{clone}/{path}", "w").write(mine.rstrip() + "\n\n" + "".join(item(there, s) + "\n" for s in missing))
    print("    copied in from the head: " + path + ": " + ", ".join(missing))
PY
  ( cd "$clone" && cargo test --locked --test tui -- --exact "$TEST" ) > "$clone.run.log" 2>&1
  rc=$?
  echo "## $name, the paste test: exit $rc; $(grep -E '^test result' "$clone.run.log" | head -1)"
  sed -n '/^---- /,/^failures:$/p' "$clone.run.log" | grep -v '^failures:$' | sed 's/^/    /' | head -16
  grep -E '^error' "$clone.run.log" | head -3 | sed 's/^/    /'
done
