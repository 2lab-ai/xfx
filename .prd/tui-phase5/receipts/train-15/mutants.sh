#!/usr/bin/env bash
# Train 15, join check 2's mutants: each a one-text change in a throwaway
# clone at the code head, the named tests run with a per-mutant timeout, the
# clone reset after. Expect: the new case fails, its neighbours pass.
# TMPDIR comes from the environment (the harness scratch root, as the gate's).
# usage: suite-lock.sh bash mutants.sh <clone>
# As run, TMPDIR was set in this file; written here as the environment's, the
# one change, so no local path is committed.
set -u
cd "$1" || exit 2
run() { # <name> <old> <new>
  local name=$1 old=$2 new=$3
  python3 -I -B - "$old" "$new" <<'PY'
import sys
p = "src/tui/shell.rs"
s = open(p).read()
old, new = sys.argv[1], sys.argv[2]
assert s.count(old) == 1, s.count(old)
open(p, "w").write(s.replace(old, new))
PY
  local start=$SECONDS
  timeout 900 cargo test --locked --lib -- held_rule in_an_amendment_are_fxs > "$name.log" 2>&1
  local rc=$?
  echo "## $name: exit $rc, $((SECONDS - start)) s"
  grep -E '^test .* \.\.\. (ok|FAILED)$|^test result' "$name.log"
  git checkout -q -- src/tui/shell.rs
}
echo "# clone at $(git rev-parse --short=8 HEAD), tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
run none 'Input::Action(Action::ControlJ) => self.ask.is_some() || self.asking(),' 'Input::Action(Action::ControlJ) => self.ask.is_some() || self.asking(),'
run M1-approval-cj-uncounted 'Input::Action(Action::ControlJ) => self.ask.is_some() || self.asking(),' 'Input::Action(Action::ControlJ) => self.ask.is_some(),'
run M2-no-cj-counted 'Input::Action(Action::ControlJ) => self.ask.is_some() || self.asking(),' 'Input::Action(Action::ControlJ) => false,'
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
