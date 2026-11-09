#!/usr/bin/env bash
# Train 11, join check 2: the bytes the preview's tick writes -- the
# preview's update, then the card's -- at the head and on
# SUBMIT-CARD-PREVIEW's own tip. join_test_catalog.rs put in as apply_join.py
# puts it, with one probe added after its byte comparison (the tick's bytes of
# each case written to $XFX_JOIN_BYTES/tick-<case>.bytes; cases 0 no catalog
# told, 1 inside the failure's hold, 2 past it) -- into the test where the
# tree holds it already (the head), else put in with it -- and on the sides without
# CATALOG-REFRESH its two helpers as those sides' types have them
# (join_helpers_catalog.rs). The test run alone (`--exact`); each case's
# sha256 printed, the head twice.
# usage: join_bytes.sh <repo> <scratch>
set -u
repo=$1 scratch=$2
here=$(cd "$(dirname "$0")" && pwd)
c="$scratch/bytes-clone"
rm -rf "$c" && git clone -q --no-local "$repo" "$c" || exit 2
t=tui::event_loop::tests::a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one
one() { # <label> <commit> <helpers or ->
  git -C "$c" checkout -q -- . && git -C "$c" clean -qfd && git -C "$c" checkout -q --detach "$2"
  python3 -I -B - "$c" "$here/join_test_catalog.rs" "$3" <<'PY'
import sys
clone, test, helpers = sys.argv[1:]
path = f"{clone}/src/tui/event_loop.rs"
text = open(path, encoding="utf-8").read()
assert text.endswith("    }\n}\n")
body = open(test, encoding="utf-8").read()
anchor = "        let unit = String::from_utf8_lossy(&ticks[0].1).into_owned();\n"
assert body.count(anchor) == 1
probe = ('        if let Ok(dir) = std::env::var("XFX_JOIN_BYTES") {\n'
         '            for (n, (_, written)) in ticks.iter().enumerate() {\n'
         '                std::fs::write(format!("{dir}/tick-{n}.bytes"), written).expect("the dump");\n'
         '            }\n'
         '        }\n')
if text.count(anchor) == 1:
    # The tree holds the test (the head): the probe goes into it.
    open(path, "w", encoding="utf-8").write(text.replace(anchor, probe + anchor))
    print("# the probe put into the test the tree holds")
else:
    body = body.replace(anchor, probe + anchor)
    if helpers != "-":
        body = open(helpers, encoding="utf-8").read() + body
    open(path, "w", encoding="utf-8").write(text[:-2] + body + "}\n")
    print("# the test put in with the probe")
PY
  local out="$scratch/bytes-$1"
  rm -rf "$out" && mkdir -p "$out"
  ( cd "$c" && XFX_JOIN_BYTES="$out" cargo test --locked --lib --no-fail-fast -- --exact "$t" ) > "$out.log" 2>&1
  local rc=$?
  echo "$1 $(git -C "$c" rev-parse --short=8 HEAD): exit $rc, $(grep -E "^test $t \.\.\. " "$out.log" | sed 's/.* \.\.\. //')"
  for f in "$out"/tick-*.bytes; do
    [ -f "$f" ] && echo "    $(basename "$f"): $(wc -c < "$f" | tr -d ' ') bytes, sha256 $(shasum -a 256 "$f" | cut -c1-16)"
  done
}
one head 820a8f15 -
one head-again 820a8f15 -
one submit-card-preview 0c203cd3 "$here/join_helpers_catalog.rs"
one merge-1 4a0b900b "$here/join_helpers_catalog.rs"
git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
echo "# bytes-clone clean after: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
for n in 0 1 2; do
  have=0 sums=""
  for s in head head-again submit-card-preview merge-1; do
    f="$scratch/bytes-$s/tick-$n.bytes"
    [ -f "$f" ] && have=$((have + 1)) && sums="$sums $(shasum -a 256 "$f" | cut -c1-16)"
  done
  echo "# tick-$n: $have of 4 dumps; $(echo $sums | tr ' ' '\n' | sort -u | grep -c .) distinct sha256 among them"
done
echo "# all cases, all four runs: $(cat "$scratch"/bytes-*/tick-*.bytes 2>/dev/null | wc -c | tr -d ' ') bytes in $(ls "$scratch"/bytes-*/tick-*.bytes 2>/dev/null | wc -l | tr -d ' ') dumps, $(shasum -a 256 "$scratch"/bytes-*/tick-*.bytes 2>/dev/null | cut -c1-64 | sort -u | wc -l | tr -d ' ') distinct"
