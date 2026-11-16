#!/usr/bin/env bash
# Train 16: train 15's mutants.sh, its mutants this train's join tests' (join
# check 2), each log written to <log-dir> rather than the clone's root, so
# the clone stays clean. Each mutant is a one-text change in a throwaway
# `git clone --no-local` at the join commit, the named tests run with a
# per-mutant timeout (900 s), the file put back after. The three join tests
# and their neighbours run each time: the unprefixed C-j matrix at the model
# question, the C-j table at the permission question, and the amendment's
# other key tables. Expect: the mutant's own join test fails, every
# neighbour passes; the unmutated run passes all.
#   MA  the second ESC no longer sets the meta prefix (META-PREFIX-DECODE's
#       stage): `ESC ESC [ D` is the plain Left
#   MB  the prefix no longer read as Alt's bit on a cursor key
#       (META-PREFIX-DECODE's `prefixed`)
#   MC  the Ctrl+letter rule refuses a report behind the prefix
#   MD  End in a draft goes to the draft's start (APPROVAL-PASTE-SPACES'
#       mapping, as the merge made it)
# TMPDIR comes from the environment (the harness scratch root, as the gate's).
# usage: suite-lock.sh bash mutants.sh <clone> <log-dir>
set -u
cd "$1" || exit 2
logs=$2
mkdir -p "$logs"
run() { # <name> <file> <old> <new>
  local name=$1 file=$2 old=$3 new=$4
  python3 -I -B - "$file" "$old" "$new" <<'PY'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
assert s.count(old) == 1, s.count(old)
open(p, "w").write(s.replace(old, new))
PY
  local start=$SECONDS
  timeout 900 cargo test --locked --lib -- meta_prefixed ctrl_j_spelling ctrl_j_answers in_an_amendment_are_fxs > "$logs/$name.log" 2>&1
  local rc=$?
  echo "## $name: exit $rc, $((SECONDS - start)) s"
  grep -E '^test .* \.\.\. (ok|FAILED)$|^test result' "$logs/$name.log"
  git checkout -q -- "$file"
}
echo "# clone at $(git rev-parse --short=8 HEAD), tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
SA='            0x1b => {
                self.stage = Stage::Esc {
                    at: now,
                    meta: true,
                }
            }'
SB='    if meta {
        mask | ALT
    } else {'
SC="    if mods & CTRL != 0 {
        if let Some(byte @ (b'a'..=b'z' | b'A'..=b'Z' | b'_')) = ascii {"
SD='            Action::End | Action::LineEnd { .. } => Action::DraftEnd { shifted: false },'
NA='            0x1b => {
                self.stage = Stage::Esc {
                    at: now,
                    meta: false,
                }
            }'
NB='    if meta {
        mask
    } else {'
NC="    if mods & CTRL != 0 && !meta {
        if let Some(byte @ (b'a'..=b'z' | b'A'..=b'Z' | b'_')) = ascii {"
ND='            Action::End | Action::LineEnd { .. } => Action::DraftStart { shifted: false },'
run none src/tui/input.rs "$SA" "$SA"
run MA-no-prefix-stage src/tui/input.rs "$SA" "$NA"
run MB-prefix-not-alt src/tui/input.rs "$SB" "$NB"
run MC-ctrl-letter-not-behind-prefix src/tui/input.rs "$SC" "$NC"
run MD-end-to-draft-start src/tui/approval_amendment.rs "$SD" "$ND"
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
