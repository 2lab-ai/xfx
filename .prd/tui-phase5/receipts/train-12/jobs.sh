#!/usr/bin/env bash
# Train 12 (train 11's jobs.sh, its parts and lists this train's), one hold of
# the suite lock per call, each part in a `git clone --no-local` of its own
# (never the worktree):
#   named   -- at <head>: INPUT-ROW-LIMIT's 11 cases (merge 4's semantic fix
#              among them) by name; the capture harness's whole suite with
#              `-v`, so each unit's test_capture.py cases are read by name
#              (join check 1);
#   mutant  -- join check 2: the head with GATE-ENV-MAINTLOCK-F's maintenance
#              fix reverted (its own m1, `sg` without `-c maintenance.auto=false
#              -c gc.auto=0`), test-gate-env.sh whole, bounded by
#              `timeout -k 10 900`; the clone restored and left clean;
#   net     -- join check 6: net.py (the lib and tui tests through a loopback
#              proxy logger) at <head>.
# usage: suite-lock.sh jobs.sh <part> <repo> <head> <scratch> [<connect_logger.py>]
set -u
part=$1 repo=$2 head=$3 scratch=$4 logger=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
clone() { # <name> <commit>
  rm -rf "$scratch/$1"
  git clone -q --no-local "$repo" "$scratch/$1" && git -C "$scratch/$1" checkout -q --detach "$2"
  echo "# $1 at $(git -C "$scratch/$1" rev-parse --short=8 HEAD)"
}
LIB=(
  # INPUT-ROW-LIMIT's 11; at_41_rows_a_page_of_the_model_menu_is_19_models holds merge 4's semantic fix
  tui::layout::tests::the_composer_may_take_half_of_fxs_content_area_plus_one_row
  tui::shell::tests::at_41_rows_a_page_of_the_help_catalog_is_19_commands
  tui::shell::tests::at_41_rows_a_page_of_the_skills_menu_is_19_skills
  tui::shell::tests::at_41_rows_a_page_of_the_at_list_is_19_paths
  tui::shell::tests::at_41_rows_a_page_of_the_resume_picker_is_19_sessions
  tui::shell::tests::at_41_rows_a_page_of_the_model_menu_is_19_models
  tui::shell::tests::at_41_rows_a_page_of_the_effort_menu_is_19_levels_round_its_five
  tui::shell::tests::at_41_rows_a_page_of_the_fork_menu_is_19_prompts
  tui::shell::tests::at_41_rows_a_page_of_the_slash_menu_is_19_commands
  tui::shell::tests::on_an_eight_row_screen_a_page_and_the_composers_cap_are_one_row
  tui::shell::tests::at_41_rows_the_composer_is_capped_at_19_rows_and_a_page_is_19_of_them
)
echo "# jobs.sh $part at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
named)
  clone named-clone "$head"
  ( cd "$scratch/named-clone" && cargo test --locked --lib --no-fail-fast -- --exact "${LIB[@]}" ) > "$scratch/named-lib.log" 2>&1
  echo "lib: exit $?"
  grep -E '^test |^test result' "$scratch/named-lib.log" | sed 's/^/    /'
  ( cd "$scratch/named-clone" && ./scripts/fxcap/test-capture.sh -v ) > "$scratch/named-capture.log" 2>&1
  echo "test-capture.sh -v: exit $?; $(grep -E '^(Ran|OK|FAILED)' "$scratch/named-capture.log" | tr '\n' ' ')"
  echo "# the clone clean after: $([ -z "$(git -C "$scratch/named-clone" status --porcelain)" ] && echo yes || echo no)"
  ;;
mutant)
  clone mutant-clone "$head"
  c="$scratch/mutant-clone"
  python3 -I -B - "$c/scripts/test-gate-env.sh" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = "\t\tgit -c core.fsmonitor=false -c maintenance.auto=false -c gc.auto=0 \"$@\"\n"
assert s.count(old) == 1, s.count(old)
open(p, "w", encoding="utf-8").write(s.replace(old, "\t\tgit -c core.fsmonitor=false \"$@\"\n"))
print("# mutant m1 (GATE-ENV-MAINTLOCK-F's own, runs-r3/m1-no-guard.diff): sg without -c maintenance.auto=false -c gc.auto=0")
PY
  git -C "$c" diff --stat | tail -1
  start=$SECONDS
  ( cd "$c" && timeout -k 10 900 ./scripts/test-gate-env.sh ) > "$scratch/mutant.log" 2>&1
  rc=$?
  echo "mutant: exit $rc, $((SECONDS - start)) s; $(tail -1 "$scratch/mutant.log")"
  grep -E '^test-gate-env: ' "$scratch/mutant.log" | grep -v ': ok$\|: skip' | cut -c1-200 | sed 's/^/    /'
  git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
  echo "# the clone clean after: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
  ;;
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
