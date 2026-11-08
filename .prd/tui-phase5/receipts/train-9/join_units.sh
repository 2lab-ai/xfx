#!/usr/bin/env bash
# Train 9, join check 2's two tests on each side alone: STOP-IN-CATALOG-WAIT's
# tip (05a0a857) and QUESTIONS-20's (3824f6ba). On a `git clone --no-local` of
# each: the tests put in unchanged (apply_join.py), each test target built
# (`cargo test --no-run`) -- does it build? -- and then, where it does not,
# the other side's test helpers it names copied in from the head (test code
# only: QUESTIONS-20's `question_batch` and `tool_output` in tests/tui.rs,
# STOP-IN-CATALOG-WAIT's `turns_logged` in worker.rs), and both tests run.
# Train 8's join_units.sh, for two tests on two targets.
# usage: join_units.sh <repo> <head> <scratch>   (inside one hold of the suite lock)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
WORKER=tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop
PANEL=a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt
for side in stop-in-catalog-wait=05a0a857 questions-20=3824f6ba; do
  name=${side%%=*} rev=${side#*=}
  clone=$scratch/alone-$name
  rm -rf "$clone"
  git clone -q --no-local "$repo" "$clone" && git -C "$clone" checkout -q "$rev"
  python3 -I -B "$here/apply_join.py" "$clone"
  for target in lib tui; do
    if [ "$target" = lib ]; then args=(--lib); else args=(--test tui); fi
    ( cd "$clone" && cargo test --locked "${args[@]}" --no-run ) > "$clone.build-$target.log" 2>&1
    rc=$?
    errors=$(grep -c '^error\[' "$clone.build-$target.log")
    echo "## $name $(git -C "$clone" rev-parse --short=8 HEAD), $target, the tests unchanged: build exit $rc, $errors errors"
    grep -A4 '^error\[' "$clone.build-$target.log" | grep -E '^(error|  *-->)' | sed 's/^/    /' | head -12
  done
  python3 -I -B - "$repo" "$head" "$clone" <<'PY'
import re, subprocess, sys
repo, head, clone = sys.argv[1:4]
def at_head(path):
    return subprocess.run(["git", "-C", repo, "show", f"{head}:{path}"], check=True,
                          capture_output=True, text=True).stdout
def item(text, start):
    # The item `start` opens, with its doc comment, through its closing brace at
    # the same indent.
    i = text.index(start)
    doc = text.rfind("\n\n", 0, i) + 2
    indent = start[: len(start) - len(start.lstrip())]
    end = text.index("\n" + indent + "}\n", i) + len(indent) + 3
    return text[doc:end]
for path, starts in (("tests/tui.rs", ["fn question_batch(", "fn tool_output("]),
                     ("src/tui/worker.rs", ["    fn turns_logged("])):
    mine = open(f"{clone}/{path}").read()
    missing = [s for s in starts if s not in mine]
    if not missing:
        continue
    there = at_head(path)
    helpers = "".join(item(there, s) + "\n" for s in missing)
    if path.endswith("worker.rs"):
        body = mine.rstrip()
        assert body.endswith("}")
        mine = body[:-1].rstrip() + "\n\n" + helpers + "}\n"
    else:
        mine = mine.rstrip() + "\n\n" + helpers
    open(f"{clone}/{path}", "w").write(mine)
    print(f"    copied in from the head: {path}: " + ", ".join(s.strip() for s in missing))
PY
  ( cd "$clone" && cargo test --locked --lib -- --exact "$WORKER" ) > "$clone.run-lib.log" 2>&1
  rc=$?
  echo "## $name, the worker test: exit $rc; $(grep -E '^test result' "$clone.run-lib.log" | head -1)"
  sed -n '/^---- /,/^failures:$/p' "$clone.run-lib.log" | grep -v '^failures:$' | sed 's/^/    /' | head -40
  grep -E '^error' "$clone.run-lib.log" | head -3 | sed 's/^/    /'
  ( cd "$clone" && cargo test --locked --test tui -- --exact "$PANEL" ) > "$clone.run-tui.log" 2>&1
  rc=$?
  echo "## $name, the panel test: exit $rc; $(grep -E '^test result' "$clone.run-tui.log" | head -1)"
  sed -n '/^---- /,/^failures:$/p' "$clone.run-tui.log" | grep -v '^failures:$' | sed 's/^/    /' | head -16
  grep -E '^error' "$clone.run-tui.log" | head -3 | sed 's/^/    /'
done
