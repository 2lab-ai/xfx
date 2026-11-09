"""Lists, each with what was decided about it:
- every comment line, parity-row sentence and README line in the unit's files
  that says "backoff", "hold" or "asks nothing" (round 4);
- every comment sentence in the same seven files, and every sentence of the
  parity row, that says "only", "not before", "never" or "once" and also names
  the catalog's reads -- the catalog, a refresh, a retry, a read again, due, a
  trigger, the model cache, a first load, a backoff or a hold (round 5): the
  words a closed list of when a read comes is written with.
usage: python3 -I sentences.py <work> <out>"""
import re, sys, os
work, out = sys.argv[1], sys.argv[2]
PAT = re.compile(r"backoff|\bhold\b|asks nothing|asked nothing", re.I)
CAT = {
 "Q4": "qualified in round 4",
 "Q3": "qualified in round 3",
 "P": "a read comes once/past the hold: stands (the hold is CatalogDue's, which carries the rule)",
 "S": "its own test's readings, none before the read's start: stands",
 "E": "the earlier-reading case itself",
 "D": "what the hold or backoff is, no promise about reads",
 "I": "an idle tick or a band with nothing to ask: not the hold",
 "N": "the /model <query> one-shot, which no backoff holds: true",
 "U": "not the catalog's hold",
 "Q5": "qualified in round 5 (a closed list given the hold and the earlier reading, or a list of triggers given a resume or a fork)",
 "R": "the due rule itself, or a pointer to it",
 "T": "an ask with no trigger: no refresh, no retry, only the first load -- true whatever the clock reads",
 "L": "the line-mode `ensure_catalog` or `/model <query>`, not the model cache's triggers",
 "F": "a quote of fx",
 "W": "not a rule of when the catalog is read",
}
# ([file|]unique part of the line) -> category, the longest part first; a line
# no part names is U
NAMED = {
 "a failed read's backoff, are counted": "D",
 "`backoff_over`, `model_cache_runtime.zig:610-618`). The runtime decides by": "E",
 "and the next trigger past the backoff reads again": "P",
 "last one began, how many have failed in a row, and when the hold": "D",
 "(`failure_backoff_base_ms`, `failure_backoff_cap_ms`,": "D",
 "The hold after the `streak`-th failure in a row": "D",
 "which is due at once ([`CatalogDue::since`]): not before the hold a": "Q3",
 "read yet, no hold.": "D",
 "once they are `wait` old: not before a failure's hold ends": "Q3",
 "a failure, at once past the hold (`:622`)": "P",
 "success clears the streak and the hold; a failure counts one more and": "Q4",
 "holds the next read off for its backoff, counted from its arrival --": "Q4",
 "When its answer arrived, if it has: what a failure's backoff counts": "D",
 "is not taken yet (`:621`); never inside a failure's backoff, for a": "Q3",
 "`src/main.zig:1996-2005`), which no backoff holds.": "N",
 "When the hold a failure put on the next read ends, if one does.": "D",
 "when the last read began, and when the hold a failure put on the next": "D",
 "come: the end of the hold the failure put on the next read, as": "P",
 "upstream's backoff holds it (`crate::provider::model::failure_backoff`).": "P",
 "failure's hold ends -- and at once, whatever held it back, at a": "Q5",
 "[`PICKER_REVALIDATE_FLOOR`] ago, and not before a failure's hold ends": "Q5",
 "failure's hold is over -- and, for a failure asking again could not": "P",
 "when that trigger is due -- a first load, a failure past its backoff,": "Q4",
 "hold, and any of them at a reading earlier than the last read's start": "Q5",
 "band then asks nothing at the keystroke.": "U",
 "failure, when the hold it put on the next read ends; for rows, when": "D",
 "([`Applied`]) -- the rows, the failure and when the hold it puts on the": "D",
 "and none is due -- a failure inside its backoff or rows read less than": "Q4",
 "([`Held::Nothing`]), past any hold a failure left on it": "P",
 "began, or, once a refresh behind them failed, when the hold it put on": "Q4",
 "when the hold the failure put on the next read ends: a `/model` before": "Q4",
 "model cache starts no load inside a failure's backoff": "Q4",
 "then opens the menu over the failure and asks nothing, as upstream's": "Q4",
 "// failed read's start (`backoff_over`, `:617`, `:621`),": "Q4",
 "(`crate::provider::model::failure_backoff`). That holds for readings": "Q4",
 "past any hold a failure put on the next read, and any of them at": "Q5",
 "hold ends; and any of them at once at a reading earlier than the last": "Q5",
 "could change -- not before a failure's hold ends either way -- and any": "Q5",
 "the next read once its hold ends -- or, for one asking again": "P",
 "began; a load that failed is read again once its backoff is over --": "P",
 "// Failed, and inside the hold the failure put on the next read": "Q4",
 "// Failed and due again -- past its hold, or at a reading earlier": "Q4",
 "`beginLoad`, `:612-623`): a failure whose hold has ended, rows that": "Q4",
 "hold has ended -- and any of them at a reading earlier than the last": "Q4",
 "failure's hold has ended ([`Self::settle_column`]).": "P",
 "failure again once its backoff is over (`beginLoad`,": "P",
 "asks nothing, here and upstream (`flushRequestedFrame` begins no": "I",
 "([`Shell::settle_column`]); a tick that owes no frame asks nothing.": "I",
 # tests
 "failure's backoff, or at a reading earlier than the failed read's": "Q4",
 "`failureBackoffMs`'s own values (`model_cache_runtime.zig:1531-1535": "D",
 "// A first load, at once; a failure, past its backoff only.": "S",
 "// Rows whose refresh failed retryably: past the backoff, whatever": "S",
 "// A success clears the streak and the hold.": "D",
 "backoff is over (`backoff_over`, `:617`): \"callers treat a reading": "E",
 "seconds and the five minutes count from -- and the hold a failure": "D",
 "@481104fd`) and the backoff a failure puts on it (`:40-54`, `:936-965`).": "D",
 "// The hold after a failure is counted on the same clock": "D",
 "// A refresh that fails, its answer's reading failing: the hold is": "E",
 "// `failureBackoffMs` (`model_cache_runtime.zig:40-54@481104fd`): after": "Q4",
 "// again whatever asks, once its backoff ends (`retry_due`,": "P",
 "// (`crate::provider::model::failure_backoff`).": "S",
 "// state, whoever started the load: inside the failure's backoff (by": "Q4",
 "// Inside its backoff: told again as it failed, and nothing read.": "S",
 "// Asked again inside the failure's backoff: told as it stands -- the": "S",
 "// same hold -- and nothing read (`beginLoad`,": "S",
 "/// backoff is over, in the background, as the band's ask starts it.": "P",
 "// A failure past its hold: activity reads it again, as upstream's": "P",
 "// `.failed` is read by any trigger past its backoff (`:621-622`);": "P",
 "// a success, and after a failure past its hold -- it reads nothing.": "S",
 "// Asked again, at once and past any hold: told as held, nothing": "S",
 "// way, whose answer is never told, and not a failure's hold, which": "S",
 "// Rows, and a refresh of them that failed: a hold on the next read.": "S",
 "// failure's backoff is over, for readings not earlier than the failed": "Q4",
 "// counted into the backoff like any other (`completeFetch`,": "D",
 "// next trigger past the backoff reads again. As `turn_loop` takes it": "P",
 "// Inside its backoff: the menu, waiting, is told the failure as held,": "S",
 "// backoff twice the first's.": "D",
 "// (`model_cache_runtime.zig:609-623@481104fd`) -- the hold's end over": "D",
 "// last read's start -- and a failure with its hold's end.": "D",
 "// 609-623@481104fd`): never inside the failure's backoff, for readings": "Q4",
 "/// When the hold a failed read put on the next one ended: long enough": "S",
 "/// ([`crate::provider::model::failure_backoff`]).": "S",
 "/// When the hold a failed read put on the next one ends: still to come": "S",
 "// The prompt was activity past the failure's hold: it asked, without": "S",
 "// the failure's backoff is over (`beginLoad`,": "P",
 "// a tick that owes no frame draws nothing and asks nothing.": "I",
 "// A frame inside the hold: asks nothing.": "S",
 "// Past the hold, an idle tick -- no frame owed -- asks nothing:": "I",
 "// @481104fd`), and the next `/model` past the failure's backoff": "P",
 "// failure's backoff (for a reading not earlier than the failed": "Q4",
 "// `/model` after the hold asks.": "P",
 "// `beginLoad` starts no load inside a failure's backoff, five seconds": "Q4",
 "// catalog again once its backoff is over (`beginLoad`,": "P",
 "// `a_read_that_finds_no_runtime_tells_the_band_its_failure_and_is_read_again_past_its_backoff`):": "P",
 "// that failure's hold is over.": "P",
 "// failed read's start, the backoff over, `model_cache_runtime.zig:": "D",
 "// backoff, for readings not earlier than the failed read's start --": "Q4",
 "// answer yet -- the launch's own read on its way -- asks nothing.": "I",
 "// Rows five minutes old, and a failure past its hold: read by": "S",
 "// it, and a failure's hold that ended in the night lets `/model` read": "S",
 "// Awake, inside the failure's hold, the menu opens over it and asks": "S",
 "// `beginLoad`'s `elapsed` and `backoff_over`,": "E",
 "// the reading, and a failure inside its hold, are both due then.": "E",
 "// way (the first load, or the failure past its hold), and with the": "P",
 "// here past the failure's hold.": "S",
 "// The tick after it owes nothing, paints nothing and asks nothing.": "I",
 "// failure just now holds the next read off for its backoff, and a": "S",
 "// sleep past it ends the hold.": "S",
 "// MODELS-REASK: a prompt past a failed catalog's hold asks the": "P",
}
FILES = ["src/provider/catalog_clock.rs", "src/provider/model.rs", "src/tui/bridge.rs",
         "src/tui/worker.rs", "src/tui/shell.rs", "src/tui/event_loop.rs", "src/interactive.rs"]
used, rows = set(), {"prod": [], "test": []}
for f in FILES:
    lines = open(os.path.join(work, f)).read().splitlines()
    test_from = next((i for i, l in enumerate(lines) if re.match(r"^\s*mod tests \{", l) and "cfg(test)" in lines[i-1]), len(lines))
    for i, l in enumerate(lines):
        s = l.strip()
        if not (s.startswith("//") and PAT.search(s)):
            continue
        cat = "U"
        for key, c in sorted(NAMED.items(), key=lambda kv: -len(kv[0])):
            path, _, part = key.rpartition("|")
            if part in s and path in ("", f):
                cat = c; used.add(key); break
        rows["test" if i >= test_from else "prod"].append(f"{f}:{i+1} [{cat}] {s}")
missing = [p for p in NAMED if p not in used]
print("named but no hit line:", missing, file=sys.stderr)
o = []
o.append("# Every comment line, parity-row sentence and README line of the unit that says")
o.append('# "backoff", "hold" or "asks nothing" -- each checked, with what was decided.')
o.append("# A promise of no read inside a failure's hold (\"asks nothing\", \"only\", \"never\",")
o.append("# \"not before\", or a closed list of when a read is due) holds only for readings")
o.append("# not earlier than the last read's start, and says so. Categories:")
for k, v in CAT.items():
    o.append(f"#   [{k}] {v}")
row = open(os.path.join(work, "docs/parity.md")).read().splitlines()[92]
sents = re.split(r"(?<=[.;])\s+(?=[A-Z*`(])", row)
o.append("")
o.append("## docs/parity.md:93, the /model row: sentences that say them")
PARITY = {
 "**After a failure, nothing reads it for": "Q3",
 "a failure's hold ends in the sleep too": "D",
 "While a failure's hold holds -- by the readings above --": "Q4",
 "asked again only at the next ask past its hold, as above": "Q4",
 "asks again once the failure's hold (above) is over": "Q4",
 "either is read again past its hold, as above": "Q4",
 "read again past its hold like any other": "P",
 "an idle session paints no frame and asks nothing": "I",
}
pused = set()
for s in sents:
    if not PAT.search(s):
        continue
    cat = "U"
    for part, c in PARITY.items():
        if part in s:
            cat = c if cat == "U" else cat + "+" + c; pused.add(part)
    m = PAT.search(s)
    a = max(0, m.start() - 110)
    o.append(f"[{cat}] ...{s[a:m.end() + 110]}...")
print("parity parts in a sentence named before:", set(PARITY) - pused, file=sys.stderr)
for kind in ("prod", "test"):
    o.append("")
    o.append(f"## source comments ({'outside tests' if kind == 'prod' else 'in the test modules'}), the unit's seven files: {len(rows[kind])} lines")
    o += rows[kind]
WORD = re.compile(r"\bonly\b|not before|\bnever\b|\bonce\b", re.I)
READS = re.compile(r"catalog|refresh|retry|\breads? (it|them|the rows|again)\b|read again|\bdue\b|REFRESH_INTERVAL|PICKER_REVALIDATE|LoadTrigger|beginLoad|model cache|warmup|first load|backoff|\bhold\b", re.I)
# (unique part of the sentence) -> category, the longest first; any other is W
DECIDED = {
 "at `at`, or at once for a reading earlier than `since`": "R",
 "so the two never disagree on whether a read is due": "R",
 "fx's callers treat a reading earlier than a stored one as due": "R",
 "a catalog in hand is read again only once it is [`REFRESH_INTERVAL`] old": "Q5",
 "they read what is held and start no refresh and no retry -- only the first load nothing has made": "T",
 "a catalog in hand is read again once the last read began [`PICKER_REVALIDATE_FLOOR`] ago": "Q5",
 "so an outage is never hammered": "F",
 "besides any reading earlier than the last read's start, which is due at once": "Q3",
 "not before the hold a failure put on the next read ends, and over rows not before": "Q3",
 "not before a failure's hold ends (`:617`)": "Q3",
 "over rows, not before `wait` from the last read's start": "Q3",
 "a first load, and a failure, at once past the hold": "Q3",
 "Loads the catalog once, if the provider advertises one": "L",
 "never inside a failure's backoff, for a reading not earlier than the last read's start": "Q3",
 "anything once they are five minutes old or once a refresh of them failed": "Q3",
 "Lets the next [`ensure_catalog`](Self::ensure_catalog) ask again after a load that failed, at once": "L",
 "once `retry_at` has come": "P",
 "or at once at a reading earlier than that start": "Q5",
 "none of upstream's triggers, so no refresh and no retry starts for it -- only the first load nothing has made": "T",
 "which starts only the first load nothing has made": "Q5",
 "Held across turns because loading the catalog costs a socket and its whole contract is \"once\"": "L",
 "`ModelSelector::ensure_catalog` attempts a load only from `NotLoaded`": "L",
 "after it, a read is the model cache's triggers'": "Q5",
 "Never a refresh of rows, and never a retry of a failure the selector holds": "T",
 "at once for a reading earlier than the last read's start ([`CatalogDue`])": "Q4",
 "the next read ends -- and, for one asking again could not change, not": "Q5",
 "an earlier reading is due at once, whatever held the read back": "Q4",
 "activity is owed the next read once its hold ends": "Q5",
 "activity may ask again once the runtime's clock says the next read is due": "R",
 "only the first load nothing has made ([`CatalogAsk::TOLD`])": "T",
 "Rows in hand are listed at once and read again behind them": "Q5",
 "a load that failed is read again once its backoff is over": "Q5",
 "xfx retries automatically.` at once, and nothing asked": "Q4",
 "asks again once the failure's hold has ended": "P",
 "acting only when one is owed": "R",
 "rows the refresh interval old, rows whose refresh failed in a way asking again could change, and a failure": "Q5",
 "rows whose refresh failed, not before its hold ends": "Q5",
 "and any of them at once at a reading earlier than the last read's start ([`CatalogDue`])": "Q5",
 "36e8c6fc moved only the prompt's hook": "R",
 # tests
 "`ensure_catalog` alone never reads a failure again": "Q4",
 "what reads it again is the model cache's triggers once they are due": "Q4",
 "`ensure_catalog` alone still asks once": "L",
 "A first load, at once": "S",
 "a failure, past its backoff only": "S",
 "Rows handed in whole, with no read on record, are due at once": "S",
 "a provider that has no catalog at all is never asked in the first place": "S",
 "\"Every trigger is user activity, so an idle session never polls": "F",
 "the hold is counted from the origin, so it is over at once and the next ask retries": "E",
 "the picker opening reads it again once the last read began five seconds ago": "S",
 "a retryable one is due again whatever asks, once its backoff ends": "P",
 "\"Only the first load for an access shows as `.loading`": "F",
 "The reading every catalog event's `due` reads as once [`undated`]": "S",
 "which reads a failed catalog again only once the failure's backoff is over, for readings not earlier": "Q4",
 "never inside the failure's backoff, for readings not earlier than the failed read's start": "Q4",
 "past it at once when asking again could change the failure (`retry_due`)": "Q4",
 "Asked again, at once and past any hold": "S",
 "which loads again once the failure's backoff is o": "P",
 "the frame that draws the column does, once the failed read is a second old": "S",
 "upstream's cache starts no load inside a failure's backoff (for a reading not earlier": "Q4",
 "activity owes them the next read once they are the refresh interval old, counted from then": "S",
 "a submitted prompt is activity, which reads a failed catalog again once its backoff is over": "P",
 "The band asks again when the runtime's clock says the next read is due": "R",
 "which reads a failed catalog again only past its backoff, for readings not earlier": "Q4",
 "no refresh and no retry, only the first load nothing has made": "T",
 "and only on a tick that owes one -- here past the failure's hold": "S",
 "activity asks again once that failure's hold is over": "P",
 # the parity row
 "**With rows in hand it lists them at once and asks for them again behind them**": "P",
 "**When the catalog is read again** is upstream's model cache's rule": "R",
 "starting it when nothing has, never a refresh": "T",
 "where fx's reads the catalog once more for it, a one-shot beside the cache": "L",
 "While a failure's hold holds -- by the readings above --": "Q4",
 "it is asked again only at the next ask past its hold, as above": "Q4",
 "asks again once the failure's hold (above) is over": "Q4",
}
dused, drows = set(), {"prod": [], "test": [], "parity": []}
def decide(s):
    for part, c in sorted(DECIDED.items(), key=lambda kv: -len(kv[0])):
        if part in s:
            dused.add(part); return c
    return "W"
for f in FILES:
    lines = open(os.path.join(work, f)).read().splitlines()
    test_from = next((i for i, l in enumerate(lines) if re.match(r"^\s*mod tests \{", l) and "cfg(test)" in lines[i-1]), len(lines))
    i = 0
    while i < len(lines):
        if not lines[i].strip().startswith("//"):
            i += 1; continue
        j = i
        while j < len(lines) and lines[j].strip().startswith("//"):
            j += 1
        text = " ".join(re.sub(r"^\s*//[/!]?\s?", "", l) for l in lines[i:j])
        for s in re.split(r"(?<=[.;:])\s+", text):
            if WORD.search(s) and READS.search(s):
                drows["test" if i >= test_from else "prod"].append(f"{f}:{i+1}-{j} [{decide(s)}] {s[:240]}")
        i = j
for s in re.split(r"(?<=[.;:])\s+", row):
    if WORD.search(s) and READS.search(s):
        m = WORD.search(s)
        drows["parity"].append(f"[{decide(s)}] ...{s[max(0, m.start() - 120):m.end() + 120]}...")
print("decided but not found:", [p for p in DECIDED if p not in dused], file=sys.stderr)
o.append("")
o.append("# Round 5: the closed-list words. Each sentence below says \"only\", \"not before\",")
o.append("# \"never\" or \"once\" and names the catalog's reads; [W] is every one that is not a rule")
o.append("# of when the catalog is read (a quote, a test catalog's own answers, the UI, Fast, skills).")
for kind, title in (("prod", "comment sentences outside tests"), ("test", "comment sentences in the test modules"), ("parity", "docs/parity.md:93, the /model row")):
    o.append("")
    o.append(f"## round 5, {title}: {len(drows[kind])}")
    o += drows[kind]
o.append("")
readme = open(os.path.join(work, ".prd/tui-phase5/receipts/catalog-refresh/README.md")).read().splitlines()
hits = [(i + 1, l.strip()) for i, l in enumerate(readme) if PAT.search(l)]
o.append(f"## .prd/tui-phase5/receipts/catalog-refresh/README.md: {len(hits)} lines, each read: a record of a round, fx's rule, or the security notes, already qualified (Q3) where they promise")
for n, l in hits:
    o.append(f"README.md:{n} {l[:200]}")
open(out, "w").write("\n".join(o) + "\n")
print(len(rows["prod"]), len(rows["test"]), len(hits))
