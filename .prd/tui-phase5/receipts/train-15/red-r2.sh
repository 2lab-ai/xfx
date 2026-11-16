#!/usr/bin/env bash
# Train 15 round 2 (new): RED for the round's new tests, each by a forced
# mutant in a throwaway `git clone --no-local` at the round's code commit,
# the named tests run with a 900 s bound, the clone put back after each.
# TMPDIR comes from the environment. Expect: the unmutated run passes all;
# each mutant fails the test named beside it.
#   MA  the rule confirmation no longer takes every key (`consume_inputs`):
#       a_rule_confirmation_and_mcps_menu_are_never_up_together
#   MB  `/mcp`'s menu no longer takes Enter (`offer_to_mcp`): the same test
#   MC  xterm's `CSI 27;6;98~` (Ctrl+Shift+B) read as nothing (`tilde_report`):
#       the_tilde_edges_and_ctrl_shift_letters_in_an_amendment_are_fxs
# Logs are written outside the clone.
# usage: suite-lock.sh bash red-r2.sh <clone> <log-dir>
set -u
cd "$1" || exit 2
logs=$2
mkdir -p "$logs"
run() { # <name> <file> <old> <new>
  local name=$1 file=$2 old=$3 new=$4
  python3 -I -B - "$file" "$old" "$new" <<'PY'
import sys
p, old, new = sys.argv[1:4]
s = open(p).read()
assert s.count(old) == 1, (p, s.count(old))
open(p, "w").write(s.replace(old, new))
PY
  local start=$SECONDS
  timeout 900 cargo test --locked --lib -- a_rule_confirmation_and_mcps_menu_are_never_up_together in_an_amendment_are_fxs > "$logs/$name.log" 2>&1
  local rc=$?
  echo "## $name: exit $rc, $((SECONDS - start)) s"
  grep -E '^test .* \.\.\. (ok|FAILED)$|^test result' "$logs/$name.log"
  grep -E "panicked at|^  left|^ right|assertion" "$logs/$name.log" | head -4 | sed 's/^/    /'
  git checkout -q -- src
}
echo "# clone at $(git rev-parse --short=8 HEAD), tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
run none src/tui/shell.rs '            if self.rule_confirm.is_some() {' '            if self.rule_confirm.is_some() {'
run MA src/tui/shell.rs '            if self.rule_confirm.is_some() {' '            if self.rule_confirm.is_some() && false {'
run MB src/tui/shell.rs '                Action::Submit => Key::Enter,' '                Action::Submit => return false,'
run MC src/tui/input.rs '        return kitty_key(u32::from(keycode), low);' '        return if keycode == 98 { ignore } else { kitty_key(u32::from(keycode), low) };'
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
