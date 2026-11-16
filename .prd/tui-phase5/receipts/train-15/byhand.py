#!/usr/bin/env python3
"""Train 15 (train 14's byhand.py, its table this train's, keyed by merge):
the conflict blocks that no rule of `blocks.py` fits, resolved by hand and
written down here, so `reproduce.sh` can redo them. The file is read in
git's diff3 style (ours, `|||||||` base, `=======` theirs). Each block in the
table is found by the sha256 (16 hex digits) of its ours, base and theirs
lines and replaced as its rule says; a block not in the table is left for
`blocks.py`; a table entry not found exactly once stops the script, changing
nothing. A rule that rewrites text says each text it finds, and stops if any
is not found exactly once.

    python3 -I -B byhand.py <merge number> <worktree-file>

Merge 1 (CATALOG-WAIT-CLOCK onto main), `src/tui/worker.rs`, one block. The
base held three tests here: E
`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
`the_history_notice_ledger_goes_with_its_conversation`, and the head of D
`a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs`.
Ours (main, IMAGE-LOSS-COPY-FX in train 14) deleted the first two -- the
history holds no notice any more -- and renamed D, its doc and its `let`
without the notice. Theirs (CATALOG-WAIT-CLOCK) moved E and D onto its
harness (the stop sent when the band hears the first turn start, the paused
clock, `Held::run`), E's assertions unchanged (its README, "The tests"), and
gave D one assert, `held` is a retry. Merged: ours' block (E and the ledger
test stay deleted: what they assert is gone), D's doc saying when the stop is
sent as theirs does ("300 ms in" is no longer true: git merged D's body onto
theirs' harness clean, below the block), and theirs' assert after ours' `let`.

Merge 2 (the decoder line onto merge 1), `src/tui/shell.rs`, two blocks in
`Shell::consume_inputs`, both kept whole:
- Ours (main, CTRLO-OTHER-PICKERS and PAGE-KEYS-SETTINGS-COLUMN's join in
  train 14) inserted the one `C-j`/`C-k` check under the full transcript
  (`composer_control`) before `/mcp`'s menu; theirs (MODIFIED-ARROWS-SCREENS)
  inserted `let menus = arrow_to_pickers(&event);` at the same place, which
  `offer_to_mcp(&menus, held)` (merged clean) reads. Ours' lines, then
  theirs': the check reads the event as before, and `menus` stands where its
  first reader needs it. (`append`: the base is empty here.)
- Ours folded the twelve `offer_to_*` calls into `Shell::offer_to_pickers`
  (CTRLO-OTHER-PICKERS, "one list of the pickers"); theirs passed each of the
  twelve `&menus` in place of `&event`. Merged: ours' one call, given
  `&menus` -- `offer_to_pickers` calls the same twelve in the same order
  (checked: theirs' twelve calls, read in order, are its body's), so every
  picker reads the held arrows as theirs had them.

`scripts/fxcap/parity-ledger.toml`, one block (the other eleven are
`blocks.py`'s `numbers`): the base's `at-ranking` cause, which ours deleted
(AT-RANK, train 14) and wrote a comment in place of; theirs
(MODIFIED-PAGE-KEYS) added `fulltranscript-response-header` right before it
and kept it. Merged: theirs' new entry whole, then ours' comment (theirs'
lines up to the base's, then ours').

Merge 3 (FULLTRANSCRIPT-RESPONSE-HEADER onto merge 2),
`scripts/fxcap/parity-ledger.toml`, one block (the other four are
`blocks.py`'s `numbers`): the base (`3f2d6e78`) holds the
`fulltranscript-response-header` cause and then `at-ranking`. Ours deleted
`at-ranking` and wrote train 14's comment in its place (merge 2, above);
theirs deleted `fulltranscript-response-header` -- the unit closes it -- and
kept `at-ranking`. Each side deleted one entry and left the other as the base
has it: merged, both deleted, ours' comment kept. The `[[cause]]` line just
above the block is the deleted entry's header (git matched it to
`at-ranking`'s on theirs' side), so the rule takes it out too.

Merge 4 (the approval stack onto merge 3), two blocks by hand (the ledger's
twelve and `tests/tui.rs`' one are `blocks.py`'s):
- `src/tui/approval.rs`, `approval::edits`' doc: ours (the decoder line,
  COMPOSER-PARAGRAPH-MOVE and COMPOSER-SUPER-ARROWS) rewrote its first
  paragraph (`ParagraphUp`, the fold of `LineStart`/`LineEnd`); theirs
  (APPROVAL-DRAFT-KEYS) kept it as the base has it and added a paragraph
  after it (no `Undo`, `Redo`, `Yank`). Ours' lines, then theirs' past the
  base's.
- `src/tui/shell.rs`, `Shell::consume_inputs`: theirs (RULE-CONFIRM-KEYS)
  put the rule confirmation's check after the approval's, before the fold,
  reading `spelled` -- the event before `unspelled`, which its base has --
  and deleted the check below the fold (merged clean). Ours (the decoder
  line, MODIFIED-ARROWS-SCREENS `8b5aacaa`) deleted `unspelled` and its
  `let (spelled, event)`: every surface above the fold reads the event as it
  came, the approval's `decide(event, ...)` included. So theirs' `spelled`
  is ours' `event` there, and the check takes `event`. Then ours' comment on
  the fold, less its parenthetical "a rule change is fx's approval,
  `input_approval_runtime.zig:128`": that named the rule change among the
  compact menus below the fold, where it is no longer read; it now reads the
  approval's translation above it (`Shell::approval_action`), which
  swallows a held Up or Down as that parenthetical said.

Merge 5 (QUESTION-CTRL-J onto merge 4), `src/tui/input.rs`, one block:
`Action::ControlJ`'s doc. Both rewrote the base's one line into the same
first line and a different clause after it -- ours (APPROVAL-CTRL-J) "Enter
at a permission question" (`approvalActionFromByte`,
`src/ui/input/runtime.zig:64@481104fd`), theirs (QUESTION-CTRL-J) "Enter at
a model question" (`questionActionFromByte`, `:141`). Merged: both
questions named, the permission question first (join MUST 2b, from
QUESTION-CTRL-J's opus review, "MUST, at the train join").
"""
import hashlib
import sys

merge, path = sys.argv[1], sys.argv[2]
OURS = "ours"


def swap(lines, old, new):
    text = "\n".join(lines)
    n = text.count(old)
    if n != 1:
        sys.exit(f"{path}: {n} places of {old[:60]!r}, not one -- read by hand")
    return text.replace(old, new).split("\n")


def d_on_the_paused_clock(o, b, t):
    out = swap(o, """\
        // `/compact` queued behind it, and a prompt after them. A Ctrl-C
        // quoting the first two, 300 ms in: the compaction the stop dropped
        // before it began is told the band once, `Dropped`, inside the stop
        // (`Queue::abandon` -> `try_send_terminal`), before the next turn
        // starts; and every place is given back.""", """\
        // `/compact` queued behind it, and a prompt after them. A Ctrl-C
        // quoting the first two, sent when the band hears the first turn
        // start, as [`first_of_two_and_a_stop`] sends one, on the paused
        // clock a retry run takes: the compaction the stop dropped before it
        // began is told the band once, `Dropped`, inside the stop
        // (`Queue::abandon` -> `try_send_terminal`), before the next turn
        // starts; and every place is given back.""")
    assert_line = '        assert!(matches!(held, Held::Retry(_)), "WindowRetry is a retry");'
    if t.count(assert_line) != 1:
        sys.exit(f"{path}: theirs' assert not found once -- read by hand")
    return out + [assert_line]


def ours_then_theirs(o, b, t):
    if b:
        sys.exit(f"{path}: the base is not empty here -- read by hand")
    return o + t


PICKERS = ["settings", "sessions", "fork", "catalog", "models", "effort", "skills",
           "column", "files", "picker", "provider_picker", "mcp_args"]


def pickers_on_menus(o, b, t):
    calls = [x.strip() for x in t if x.strip().startswith("if self.offer_to_")]
    want = [f"if self.offer_to_{p}(&menus) {{" for p in PICKERS]
    if calls != want:
        sys.exit(f"{path}: theirs' calls are not the twelve on `&menus` -- read by hand")
    body = open(path, encoding="utf-8").read()
    start = body.index("    fn offer_to_pickers(&mut self, event: &Input) -> bool {")
    inner = body[start:body.index("\n    }\n", start)]
    if [x.strip(" |") for x in inner.split("\n")[1:]] != [f"self.offer_to_{p}(event)" for p in PICKERS]:
        sys.exit(f"{path}: `offer_to_pickers` is not the twelve in theirs' order -- read by hand")
    return swap(o, "if self.offer_to_pickers(&event) {", "if self.offer_to_pickers(&menus) {")


def both_deleted_ours_comment(o, b, t):
    import re
    nums = re.compile(r":\d+(?:-\d+)?")
    first = b.index("[[cause]]")
    if [nums.sub(":N", x) for x in o[:first - 1]] != [nums.sub(":N", x) for x in b[:first - 1]]:
        sys.exit(f"{path}: ours' first entry is not the base's -- read by hand")
    if t != b[first + 1:]:
        sys.exit(f"{path}: theirs is not the base less its first entry -- read by hand")
    if o[first - 1] != "" or not all(x.startswith("# ") for x in o[first:]):
        sys.exit(f"{path}: ours is not the first entry, a blank and a comment -- read by hand")
    return ("drop [[cause]] above", o[first:])


def ours_then_theirs_tail(o, b, t):
    if t[:len(b)] != b:
        sys.exit(f"{path}: theirs is not the base and lines after it -- read by hand")
    return o + t[len(b):]


def rule_confirm_on_event(o, b, t):
    check = [
        "            if self.rule_confirm.is_some() {",
        "                self.offer_to_rule_confirm(spelled, now);",
        "                continue;",
        "            }",
    ]
    k = next(i for i in range(len(t)) if t[i:i + 4] == check)
    if t[k + 4:] != b:
        sys.exit(f"{path}: theirs is not its check and then the base's comment -- read by hand")
    mine = t[:k] + swap(check, "offer_to_rule_confirm(spelled, now)", "offer_to_rule_confirm(event, now)")
    fold = swap(o, """\
            // `app_input_runtime.zig:2259-2263@481104fd`; a rule change is
            // fx's approval, `input_approval_runtime.zig:128`).""", """\
            // `app_input_runtime.zig:2259-2263@481104fd`).""")
    return mine + fold


def control_j_both(o, b, t):
    if o[0] != t[0] or len(o) != 3 or len(t) != 3:
        sys.exit(f"{path}: the two docs do not share their first line -- read by hand")
    want_o = ["    /// Enter at a permission question (`approvalActionFromByte`,",
              "    /// `src/ui/input/runtime.zig:64@481104fd`; APPROVAL-CTRL-J)."]
    want_t = ["    /// Enter at a model question (`questionActionFromByte`,",
              "    /// `src/ui/input/runtime.zig:141@481104fd`; QUESTION-CTRL-J)."]
    if o[1:] != want_o or t[1:] != want_t:
        sys.exit(f"{path}: the two clauses are not the ones read -- read by hand")
    return [o[0],
            "    /// Enter at a permission question (`approvalActionFromByte`,",
            "    /// `src/ui/input/runtime.zig:64@481104fd`; APPROVAL-CTRL-J) and at a model",
            "    /// question (`questionActionFromByte`, `src/ui/input/runtime.zig:141@481104fd`;",
            "    /// QUESTION-CTRL-J)."]


def inserted_before_ours(o, b, t):
    if t[len(t) - len(b):] != b:
        sys.exit(f"{path}: theirs is not lines inserted before the base -- read by hand")
    return t[:len(t) - len(b)] + o


TABLE = {
    ("1", "src/tui/worker.rs"): [
        (("49fc207cabd69b5a", "6c3104b7cece42c3", "3ee98832fc7a52f8"), d_on_the_paused_clock),
    ],
    ("2", "src/tui/shell.rs"): [
        (("c19b30b2addb45a7", "e3b0c44298fc1c14", "92b616b3538f5452"), ours_then_theirs),
        (("aef44bfd6659dc4d", "bfb8dfe5699e5688", "5ace6a368358eb55"), pickers_on_menus),
    ],
    ("2", "scripts/fxcap/parity-ledger.toml"): [
        (("319c8e167daba94a", "3cb5b00722cc36c1", "57a7c439898b3750"), inserted_before_ours),
    ],
    ("4", "src/tui/approval.rs"): [
        (("496dfa7e2a024332", "9f8aac59c6d6df95", "1dbe5e846cd89e62"), ours_then_theirs_tail),
    ],
    ("4", "src/tui/shell.rs"): [
        (("d30544ec86540861", "aecae95871a0bc33", "a1acc105ffae6c7f"), rule_confirm_on_event),
    ],
    ("5", "src/tui/input.rs"): [
        (("7ebb6e3ce214c386", "8633d9101bf630f4", "aeca2f601e7c48ae"), control_j_both),
    ],
    ("3", "scripts/fxcap/parity-ledger.toml"): [
        (("7763ab21cfaad498", "70edb4ff92ed6c50", "81fd7f6e1d6d702f"), both_deleted_ours_comment),
    ],
}


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]


table = next((v for (m, k), v in TABLE.items() if m == merge and path.endswith(k)), None)
if table is None:
    sys.exit(f"merge {merge} {path}: no table")
found = {sides: 0 for sides, _ in table}
lines = open(path, encoding="utf-8").read().split("\n")
out, i, said = [], 0, []
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    bar = next(j for j in range(i, len(lines)) if lines[j].startswith("||||||| "))
    mid = next(j for j in range(bar, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    o, b, t = lines[i + 1:bar], lines[bar + 1:mid], lines[mid + 1:end]
    sides = (h(o), h(b), h(t))
    rule = dict(table).get(sides)
    if rule is None:
        out.extend(lines[i:end + 1])
    else:
        found[sides] += 1
        merged = o if rule == OURS else t if rule == "theirs" else rule(o, b, t)
        name = rule if isinstance(rule, str) else rule.__name__
        if isinstance(merged, tuple):
            if out[-1] != "[[cause]]":
                sys.exit(f"{path}:{len(out)}: not a `[[cause]]` line above the block -- read by hand")
            out.pop()
            name += f", {merged[0]}"
            merged = merged[1]
        said.append(f"{path}:{len(out) + 1}: by hand, {name} "
                    f"(ours {len(o)} lines, base {len(b)}, theirs {len(t)} -> {len(merged)})")
        out.extend(merged)
    i = end + 1
if any(n != 1 for n in found.values()):
    sys.exit(f"{path}: table entries found {list(found.values())}, not once each -- read by hand")
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {path}: {len(said)} blocks resolved by hand")
