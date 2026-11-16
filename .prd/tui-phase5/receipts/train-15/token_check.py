#!/usr/bin/env python3
"""What each merge kept of docs/parity.md and the ledger, checked against
both sides and their merge base.

    python3 -I -B token_check.py <repo> <merge>...

For each merge commit M (first parent = ours, second = theirs, base = git
merge-base of the two):

- docs/parity.md, row by row (a table row is keyed by its first cell, in
  order of appearance; every other line by its text). Each row is split into
  whitespace tokens, as multisets. Expected = base + what ours added + what
  theirs added - what ours deleted - what theirs deleted. The row passes when
  M's row is exactly that multiset: every token a side added since the base
  is in it, and no token a side deleted came back.
- the ledger, entry by entry (`[[table]]` keyed by its `id`, every field
  compared): an entry one side changed is that side's in M, an entry neither
  changed is the base's, an entry one side deleted (the other not changing
  it) is gone, an entry one side added is in M as that side wrote it. Comment
  lines: every one a side added is in M; every one a side deleted is not.
  Train 4: an entry both sides changed differently passes when the two
  sides and M are the same entry once every `path:line` number is masked
  (`path:N`) -- a conflict in reference numbers alone. The numbers M holds
  are not checked here: resolve_refs.py and repointed.py check each one
  against the code it names.
- Train 5: the order of docs/parity.md's table rows. Every two rows a side
  has in one order, and which M holds, are in that order in M: a row a side
  inserted sits where that side put it relative to every row both kept.
- Train 6: the clauses of a row both sides changed. For such a row, every
  run of tokens a side wrote in place of the base's (`difflib` opcodes
  `insert` and `replace`, whitespace tokens, against the base's row), with
  the side's own token on either side of it, is in M's row whole, as one
  contiguous run: a clause a side wrote is not split or reordered inside.
  The multiset check alone does not look at the order of tokens inside a
  row, and merges 2 and 3 of this train each resolved two such rows word by
  word.
- Train 6, round 2: the tokens' order in every row a side changed. For each
  side, the tokens of its row the merge must keep -- every token it wrote,
  and every base token it kept that the other side kept too -- appear in
  M's row in that side's order (a subsequence of M's row). With the
  clause rule that is "whole, contiguous and in place": a clause moved
  whole, anchors and all, keeps its multiset and its contiguity and breaks
  this order. A row only one side changed must so be that side's row,
  token for token.

Prints one line per difference from the expectation, then a summary per
merge; exit 1 if any.

    python3 -I -B token_check.py <repo> --self-test <merge>

The negative control: the merge's own texts with one token a side added
taken out of its row, one token a side deleted put back, one ledger entry a
side added dropped, one entry a side deleted restored, and one comment line
dropped -- each must be found. Train 5 adds, for every docs/parity.md row
a side changed, M's row put back as the other side has it (a side's change
lost), for every row a side added, that row dropped, and for every row a
side added next to a row the other side changed, the two swapped (the order
rule) -- each must be found. Train 6 adds, for every row both sides changed,
M's row as ours has it and as theirs has it (one side's clauses lost), and
the longest run a side wrote with its first two tokens swapped (the clause
rule) -- each must be found. Round 2 adds, for every row both sides changed,
its longest clause moved whole with its anchors to the start of the row's
last cell (the key, the multiset and the contiguity unchanged; only the
order rule sees it),
and for the first row one side alone changed, two of its tokens swapped --
each must be found. Merge 5 (ARGROW-CACHE), whose sides changed neither
file: a row neither side changed must be the base's exactly (round 1's rules
let a reordering inside it by), and its control plants a token dropped, two
tokens swapped, two rows swapped, a ledger entry and a comment line dropped.

Train 7 adds two rules for references both sides re-pointed on their own
branches (train 4's numbers-only rule, carried to where train 7 met it):

- docs/parity.md: a token of a row that both sides replaced, one token for
  one at the same place in the base's row, each side's token and the base's
  equal once every `:N` (a line number or a range) is masked -- one
  reference both sides re-pointed to their own branch's line -- is compared
  with its numbers masked, in the base, both sides and M (M's token is the
  one at that place when M's row is lined up with theirs'). So a side whose
  only edit to a row was such a re-point did not change the row, and every
  rule above applies as for a row the other side alone changed. The number
  M holds is repointed.py's to check against the code.
- the ledger: an entry one side deleted and the other changed only in
  `path:line` numbers is gone from M; an entry one side changed and the
  other changed only in `path:line` numbers is the first side's in M, its
  numbers aside.

The negative control adds, for each such parity token, M's token given
another path (found), and dropped (found); for each such ledger entry, the
entry put back as the side that kept it has it (found).
"""
import difflib
import collections
import re
import subprocess
import sys
import tomllib

REF = re.compile(r"\b((?:src|scripts|tests|docs)/[A-Za-z0-9_./-]+\.(?:rs|sh|py|md|toml|ts|json)):(\d+)(?:-(\d+))?(?!\d|-\d|@)")


def masked(entry):
    if isinstance(entry, dict):
        return {k: masked(v) for k, v in entry.items()}
    if isinstance(entry, list):
        return [masked(v) for v in entry]
    if isinstance(entry, str):
        return REF.sub(lambda m: m.group(1) + ":N", entry)
    return entry

# Train 7: a reference's numbers, masked: `worker.rs:3835` and `:3524` alike.
NUMS = re.compile(r":\d+(?:-\d+)?")


def num_masked(token):
    return NUMS.sub(":N", token)


def one_for_one(a, b):
    """{index in a: index in b} for every token `b` wrote in place of one of
    `a`'s, one for one (difflib `replace` spans of equal length)."""
    out = {}
    for op, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if op == "replace" and i2 - i1 == j2 - j1:
            out.update({i1 + k: j1 + k for k in range(i2 - i1)})
    return out


def aligned(a, b):
    """{index in a: index in b} for every token of `a` that `b` keeps or
    writes one for one in its place."""
    out = one_for_one(a, b)
    for i, j, size in difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks():
        out.update({i + k: j + k for k in range(size)})
    return out


def masked_repoints(base, ours, theirs, merged, notes=None):
    """docs/parity.md's four texts with every reference both sides
    re-pointed (one token for one at the same place, the three equal once
    numbers are masked, the two sides' numbers different) masked, at its
    place, in every text; M's token is the one at theirs' place, if it is
    that reference with its numbers masked."""
    texts = [t.split("\n") for t in (base, ours, theirs, merged)]
    where = []
    for text in texts:
        where.append({line.split("|")[1].strip(): n for n, line in enumerate(text)
                      if line.startswith("| ") and line.count("|") > 3})
    for key, nb in where[0].items():
        if not all(key in w for w in where[1:]):
            continue
        b, o, t, m = (texts[k][where[k][key]].split(" ") for k in range(4))
        so, st = one_for_one(b, o), one_for_one(b, t)
        tm = aligned(t, m)
        hits = []
        for i in sorted(so.keys() & st.keys()):
            jo, jt = so[i], st[i]
            if o[jo] != t[jt] and num_masked(o[jo]) == num_masked(t[jt]) == num_masked(b[i]):
                hits.append((i, jo, jt, tm.get(jt)))
        for i, jo, jt, jm in hits:
            form = num_masked(b[i])
            got = m[jm] if jm is not None else None
            if notes is not None:
                notes.append(f"  parity row {key!r}: {b[i]!r} re-pointed by both sides (ours {o[jo]!r}, "
                             f"theirs {t[jt]!r}), M {got!r} -- compared as {form!r}")
            b[i], o[jo], t[jt] = form, form, form
            if jm is not None and num_masked(m[jm]) == form:
                m[jm] = form
        for k, row in enumerate((b, o, t, m)):
            texts[k][where[k][key]] = " ".join(row)
    return ["\n".join(t) for t in texts]


PARITY = "docs/parity.md"
LEDGER = "scripts/fxcap/parity-ledger.toml"


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def show(repo, commit, path):
    return git(repo, "show", f"{commit}:{path}")


def rows(text):
    keyed = collections.OrderedDict()
    seen = collections.Counter()
    for line in text.splitlines():
        if line.startswith("|"):
            key = line.split("|")[1].strip()
            seen[key] += 1
            keyed[("row", key, seen[key])] = line
        else:
            seen[("line", line)] += 1
            keyed[("line", line, seen[("line", line)])] = line
    return keyed


def tokens(line):
    return collections.Counter((line or "").split())


def clause_runs(base_row, side_row):
    """Each run of tokens `side_row` wrote in place of `base_row`'s, with the
    side's own token on either side of it when it has one."""
    a, b = base_row.split(), side_row.split()
    runs = []
    ops = difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes()
    for op, i1, i2, j1, j2 in ops:
        if op in ("insert", "replace") and j2 > j1:
            runs.append(b[max(0, j1 - 1):j2 + 1])
    return runs


def kept_base(base_row, side_row):
    """The indices of `base_row`'s tokens `side_row` kept."""
    sm = difflib.SequenceMatcher(None, base_row, side_row, autojunk=False)
    return {a + k for a, _, size in sm.get_matching_blocks() for k in range(size)}


def owed_in_order(base_row, side_row, other_row):
    """The tokens of `side_row` the merge must keep, in `side_row`'s order:
    every token the side wrote, and every base token it kept that the other
    side kept too."""
    other = kept_base(base_row, other_row)
    out = []
    sm = difflib.SequenceMatcher(None, base_row, side_row, autojunk=False)
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            out += [side_row[j1 + k] for k in range(i2 - i1) if i1 + k in other]
        else:
            out += side_row[j1:j2]
    return out


def subsequence(seq, row):
    it = iter(row)
    return all(token in it for token in seq)


def contiguous(run, row):
    n = len(run)
    return any(row[i:i + n] == run for i in range(len(row) - n + 1))


def check_parity(base, ours, theirs, merged, out):
    notes = []
    base, ours, theirs, merged = masked_repoints(base, ours, theirs, merged, notes)
    out.extend(notes)
    b, o, t, m = rows(base), rows(ours), rows(theirs), rows(merged)
    bad = 0
    changed = []
    for key in list(b) + [k for k in list(o) + list(t) + list(m) if k not in b]:
        if key in [c[0] for c in changed]:
            continue
        lb, lo, lt, lm = (x.get(key) for x in (b, o, t, m))
        if lb == lo == lt == lm:
            continue
        cb, co, ct, cm = tokens(lb), tokens(lo), tokens(lt), tokens(lm)
        # Train 6, merge 5: a row neither side changed is the base's, token
        # for token and in order (the multiset alone lets a reordering by).
        if lb is not None and lo == lb and lt == lb and lm != lb:
            bad += 1
            label = key[1] if key[0] == "row" else ("<line> " + key[1][:40])
            out.append(f"  PARITY {label!r}: changed by neither side, not the base's")
            changed.append((key, []))
            continue
        want = cb + (co - cb) + (ct - cb)
        want = want - (cb - co) - (cb - ct)
        sides = [name for name, line in (("ours", lo), ("theirs", lt)) if line != lb]
        label = key[1] if key[0] == "row" else ("<line> " + key[1][:40])
        if cm != want:
            bad += 1
            out.append(f"  PARITY {label!r}: missing {dict(want - cm)} extra {dict(cm - want)}")
        elif len(sides) == 2 and lb is not None and lm is not None:
            runs = clause_runs(lb, lo) + clause_runs(lb, lt)
            split = [r for r in runs if not contiguous(r, lm.split())]
            if split:
                bad += 1
                out.append(f"  PARITY {label!r}: {len(split)} of {len(runs)} clauses not whole, "
                           f"first {' '.join(split[0])[:80]!r}")
            else:
                added = sum(((co - cb) + (ct - cb)).values())
                deleted = sum(((cb - co) + (cb - ct)).values())
                out.append(f"  parity row {label!r}: changed by ours+theirs; {added} tokens added, {deleted} "
                           f"deleted, all as expected; {len(runs)} clauses (each with its "
                           f"side's token either side) whole and contiguous; each side's tokens in its order")
        else:
            added = sum(((co - cb) + (ct - cb)).values())
            deleted = sum(((cb - co) + (cb - ct)).values())
            out.append(f"  parity row {label!r}: changed by {'+'.join(sides) or 'none'}; "
                       f"{added} tokens added, {deleted} deleted, all as expected, in order")
        # Round 2: each side's owed tokens, in its order.
        if lb is not None and lm is not None and cm == want:
            for name, line, other in (("ours", lo, lt), ("theirs", lt, lo)):
                if line is None or line == lb or other is None:
                    continue
                if not subsequence(owed_in_order(lb.split(), line.split(), other.split()), lm.split()):
                    bad += 1
                    out.append(f"  PARITY {label!r}: {name}'s tokens not in {name}'s order")
        changed.append((key, sides))
    # Train 5: the rows' order. For each side, the table rows it holds that M
    # holds too must come in M in the side's order.
    at = {k: i for i, k in enumerate(m) if k[0] == "row"}
    before = bad
    for name, side in (("ours", o), ("theirs", t)):
        held = [k for k in side if k[0] == "row" and k in at]
        for first, second in zip(held, held[1:]):
            if at[first] > at[second]:
                bad += 1
                out.append(f"  PARITY order: {first[1]!r} before {second[1]!r} on {name}, after it in the merge")
    if bad == before:
        out.append("  parity order: every row in each side's order")
    return bad


def entries(text):
    data = tomllib.loads(text)
    keyed = {}
    for table, value in data.items():
        if isinstance(value, list):
            for entry in value:
                if isinstance(entry, dict) and "id" in entry:
                    keyed[(table, entry["id"])] = entry
        else:
            keyed[(table, None)] = value
    return keyed


def comments(text):
    return collections.Counter(line for line in text.splitlines() if line.lstrip().startswith("#"))


def check_ledger(base, ours, theirs, merged, out):
    b, o, t, m = entries(base), entries(ours), entries(theirs), entries(merged)
    bad = 0
    for key in sorted(set(b) | set(o) | set(t) | set(m), key=str):
        eb, eo, et, em = (x.get(key) for x in (b, o, t, m))
        if eo == eb:
            want, why = et, "theirs" if et != eb else "base"
        elif et == eb:
            want, why = eo, "ours"
        elif eo == et:
            want, why = eo, "both alike"
        elif eo is None and eb is not None and masked(et) == masked(eb):
            # Train 7: deleted by ours, its numbers alone changed by theirs.
            want, why = None, "ours (theirs changed only its path:line numbers)"
        elif et is None and eb is not None and masked(eo) == masked(eb):
            want, why = None, "theirs (ours changed only its path:line numbers)"
        elif em is not None and masked(eo) == masked(et) == masked(em):
            out.append(f"  ledger {key[0]} {key[1]}: both sides changed its path:line numbers alone; "
                       f"kept, the numbers re-pointed (resolve_refs.py, repointed.py)")
            continue
        elif em is not None and masked(et) == masked(eb) and masked(em) == masked(eo):
            # Train 7 (after the both-numbers rule): theirs changed only its
            # numbers, ours more; ours' entry, its
            # numbers re-pointed.
            out.append(f"  ledger {key[0]} {key[1]}: changed by ours, its path:line numbers alone by theirs; "
                       f"ours' entry, the numbers re-pointed (repointed.py)")
            continue
        elif em is not None and masked(eo) == masked(eb) and masked(em) == masked(et):
            out.append(f"  ledger {key[0]} {key[1]}: changed by theirs, its path:line numbers alone by ours; "
                       f"theirs' entry, the numbers re-pointed (repointed.py)")
            continue
        else:
            out.append(f"  LEDGER {key}: both sides changed it differently -- read by hand")
            bad += 1
            continue
        if em != want:
            bad += 1
            out.append(f"  LEDGER {key}: not as {why} has it")
        elif why not in ("base",):
            state = "deleted" if want is None else ("added" if eb is None else "changed")
            out.append(f"  ledger {key[0]} {key[1]}: {state} by {why}, kept so")
    cb, co, ct, cm = comments(base), comments(ours), comments(theirs), comments(merged)
    want = cb + (co - cb) + (ct - cb) - (cb - co) - (cb - ct)
    if cm != want:
        bad += 1
        out.append(f"  LEDGER comments: missing {dict(want - cm)} extra {dict(cm - want)}")
    else:
        out.append(f"  ledger comments: {sum(((co - cb) + (ct - cb)).values())} added, "
                   f"{sum(((cb - co) + (cb - ct)).values())} deleted, all as expected")
    return bad


def self_test_unchanged(pb, po, pt, pm, lb, lo, lt, lm):
    """Train 6, merge 5: the negative control for a merge whose sides changed
    neither file since their base. The merged texts must then be the base's;
    each case plants one change in M and must be found: a token dropped from a
    row, two tokens of a row swapped (the multiset unchanged), two adjacent
    rows swapped, a ledger entry dropped, a comment line dropped."""
    found = cases = 0
    lines = pm.split("\n")
    table = [i for i, line in enumerate(lines) if line.startswith("| ") and line.count("|") > 3]
    i = table[len(table) // 2]
    words = lines[i].split(" ")
    j = next(k for k in range(3, len(words) - 1) if words[k] != words[k + 1])
    dropped = lines[:i] + [" ".join(words[:j] + words[j + 1:])] + lines[i + 1:]
    swapped_words = words[:j] + [words[j + 1], words[j]] + words[j + 2:]
    swapped = lines[:i] + [" ".join(swapped_words)] + lines[i + 1:]
    k = next(n for n in table if n + 1 in table)
    rows_swapped = lines[:k] + [lines[k + 1], lines[k]] + lines[k + 2:]
    for name, text in (("a token of a row neither side changed, dropped", dropped),
                       ("two tokens of a row neither side changed, swapped", swapped),
                       ("two adjacent rows neither side changed, swapped", rows_swapped)):
        out = []
        bad = check_parity(pb, po, pt, "\n".join(text), out)
        print(f"  self-test parity, {name}: {'found' if bad else 'MISSED'}")
        found += bool(bad)
        cases += 1
    ident = next(iter(k for k in entries(lm) if k[1] is not None))
    start = lm.index(f'id = "{ident[1]}"')
    head = lm.rindex(f"[[{ident[0]}]]", 0, start)
    end = lm.find("\n[[", start)
    out = []
    bad = check_ledger(lb, lo, lt, lm[:head] + lm[end + 1:], out)
    print(f"  self-test ledger, an entry neither side changed ({ident[1]}) dropped: {'found' if bad else 'MISSED'}")
    found += bool(bad)
    line = next(line for line in lm.splitlines() if line.lstrip().startswith("#"))
    out = []
    bad = check_ledger(lb, lo, lt, lm.replace(line + "\n", "", 1), out)
    print(f"  self-test ledger, a comment line dropped: {'found' if bad else 'MISSED'}")
    found += bool(bad)
    return found, cases + 2


def self_test_repoints(pb, po, pt, pm, lb, lo, lt, lm):
    """Train 7's controls. For every docs/parity.md reference both sides
    re-pointed: M's token given another path, and dropped. For every ledger
    entry one side deleted and the other changed only in numbers: put back
    as that side has it. For every entry one side changed and the other only
    in numbers: M's entry with a word of its `why` (or `reason`) changed."""
    found = cases = 0
    notes = []
    masked_repoints(pb, po, pt, pm, notes)
    lines = pm.split("\n")
    for note in notes:
        key = note.split("parity row ", 1)[1].split(": ", 1)[0][1:-1]
        got = note.rsplit(", M ", 1)[1].split(" -- compared as", 1)[0][1:-1]
        n = next(i for i, line in enumerate(lines)
                 if line.startswith("| ") and line.split("|")[1].strip() == key)
        words = lines[n].split(" ")
        at = words.index(got)
        path = re.search(r"[A-Za-z_]+\.rs", got)
        other = got.replace(path.group(0), "elsewhere.rs") if path else "`elsewhere.rs" + got
        for name, row in (("given another path", words[:at] + [other] + words[at + 1:]),
                          ("dropped", words[:at] + words[at + 1:])):
            mutated = "\n".join(lines[:n] + [" ".join(row)] + lines[n + 1:])
            out = []
            bad = check_parity(pb, po, pt, mutated, out)
            print(f"  self-test parity, a reference both sides re-pointed ({got!r} in {key[:40]!r}) {name}: "
                  f"{'found' if bad else 'MISSED'}")
            found += bool(bad)
            cases += 1
    eb, eo, et, em = entries(lb), entries(lo), entries(lt), entries(lm)
    for key in sorted(set(eb), key=str):
        if key[1] is None:
            continue
        for gone, kept, src in ((eo, et, lt), (et, eo, lo)):
            if key in gone or key not in kept or masked(kept[key]) != masked(eb[key]) or kept[key] == eb[key]:
                continue
            start = src.index(f'id = "{key[1]}"')
            head = src.rindex(f"[[{key[0]}]]", 0, start)
            end = src.find("\n[[", start)
            out = []
            bad = check_ledger(lb, lo, lt, lm + "\n" + src[head:end] + "\n", out)
            print(f"  self-test ledger, an entry one side deleted and the other re-pointed ({key[1]}) back: "
                  f"{'found' if bad else 'MISSED'}")
            found += bool(bad)
            cases += 1
    for key in sorted(em, key=str):
        if key[1] is None or key not in eb:
            continue
        one = masked(eo.get(key)) == masked(eb[key]) and eo.get(key) != eb[key] and masked(et.get(key)) != masked(eb[key])
        two = masked(et.get(key)) == masked(eb[key]) and et.get(key) != eb[key] and masked(eo.get(key)) != masked(eb[key])
        if not (one or two):
            continue
        start = lm.index(f'id = "{key[1]}"')
        end = lm.find("\n[[", start)
        body = lm[start:end]
        field = "why = " if "\nwhy = " in body else "reason = "
        changed = body.replace(field + '"', field + '"X', 1)
        out = []
        bad = check_ledger(lb, lo, lt, lm[:start] + changed + lm[end:], out)
        print(f"  self-test ledger, an entry one side changed and the other re-pointed ({key[1]}) with a word "
              f"changed: {'found' if bad else 'MISSED'}")
        found += bool(bad)
        cases += 1
    return found, cases


def self_test(repo, merge):
    ours, theirs = git(repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
    base = git(repo, "merge-base", ours, theirs).strip()
    pb, po, pt, pm = (show(repo, c, PARITY) for c in (base, ours, theirs, merge))
    lb, lo, lt, lm = (show(repo, c, LEDGER) for c in (base, ours, theirs, merge))
    found = 0
    # Train 7: the controls for its two rules, on the raw texts; then every
    # case below chooses and plants on the texts with each reference both
    # sides re-pointed masked, as check_parity reads them.
    found7, cases7 = self_test_repoints(pb, po, pt, pm, lb, lo, lt, lm)
    found += found7
    pb, po, pt, pm = masked_repoints(pb, po, pt, pm)
    # A row a side changed: one of the tokens it added taken out, then one
    # it deleted put back.
    b, o, t = rows(pb), rows(po), rows(pt)
    key = next((k for k in b if k[0] == "row" and (o.get(k) != b[k] or t.get(k) != b[k])), None)
    if key is None:
        return self_test_unchanged(pb, po, pt, pm, lb, lo, lt, lm)
    side = o[key] if o.get(key) != b[key] else t[key]
    added = next(iter(tokens(side) - tokens(b[key])))
    gone = next(iter(tokens(b[key]) - tokens(side)), None)
    cases = [("a token a side added, dropped", side.replace(" " + added, "", 1))]
    if gone:
        cases.append(("a token a side deleted, back", side + " " + gone))
    for name, row in cases:
        out = []
        bad = check_parity(pb, po, pt, pm.replace(side, row, 1), out)
        print(f"  self-test parity, {name}: {'found' if bad else 'MISSED'}")
        found += bool(bad)
    eb, eo, et, em = entries(lb), entries(lo), entries(lt), entries(lm)
    added_ids = [k for k in em if k not in eb]
    deleted_ids = [k for k in eb if k not in em]
    if added_ids:
        table, ident = added_ids[0]
        start = lm.index(f'id = "{ident}"')
        head = lm.rindex(f"[[{table}]]", 0, start)
        end = lm.find("\n[[", start)
        out = []
        bad = check_ledger(lb, lo, lt, lm[:head] + lm[end + 1:], out)
        print(f"  self-test ledger, an added entry ({ident}) dropped: {'found' if bad else 'MISSED'}")
        found += bool(bad)
    if deleted_ids:
        table, ident = deleted_ids[0]
        start = lb.index(f'id = "{ident}"')
        head = lb.rindex(f"[[{table}]]", 0, start)
        end = lb.find("\n[[", start)
        out = []
        bad = check_ledger(lb, lo, lt, lm + "\n" + lb[head:end] + "\n", out)
        print(f"  self-test ledger, a deleted entry ({ident}) back: {'found' if bad else 'MISSED'}")
        found += bool(bad)
    line = next(line for line in lm.splitlines() if line.lstrip().startswith("#"))
    out = []
    bad = check_ledger(lb, lo, lt, lm.replace(line + "\n", "", 1), out)
    print(f"  self-test ledger, a comment line dropped: {'found' if bad else 'MISSED'}")
    found += bool(bad)
    # Train 4: an entry whose conflict was in reference numbers alone, with
    # a word of it changed in M, and with one of its references dropped.
    both = [k for k in em if eo.get(k) not in (eb.get(k), et.get(k)) and et.get(k) != eb.get(k)
            and masked(eo.get(k)) == masked(et.get(k)) == masked(em[k])]
    extra = 0
    if both:
        table, ident = both[0]
        start = lm.index(f'id = "{ident}"')
        end = lm.find("\n[[", start)
        body = lm[start:end]
        ref = REF.search(body)
        for name, changed in (
            ("a word changed", body.replace(" = ", " = ", 1).replace('owner = "', 'owner = "X', 1)),
            ("a reference dropped", body.replace(ref.group(0), "", 1)),
        ):
            out = []
            bad = check_ledger(lb, lo, lt, lm[:start] + changed + lm[end:], out)
            print(f"  self-test ledger, a numbers-only entry ({ident}) with {name}: {'found' if bad else 'MISSED'}")
            found += bool(bad)
            extra += 1
    # Train 5: every parity row a side changed, with M's row put back as the
    # other side has it; every row a side added, dropped; every row a side
    # added next to a row the other side changed, the two swapped.
    m_rows = rows(pm)
    lines = pm.split("\n")
    for key in [k for k in m_rows if k[0] == "row"]:
        lo, lt, lb, row = o.get(key), t.get(key), b.get(key), m_rows[key]
        if lb is None and (lo is None) != (lt is None):
            mutated = "\n".join(line for line in lines if line != row)
            name = f"a row a side added ({key[1][:40]!r}) dropped"
        elif lb is not None and (lo != lb) != (lt != lb):
            other = lt if lo != lb else lo
            if other is None or other == row:
                continue
            mutated = pm.replace(row, other, 1)
            name = f"a row a side changed ({key[1][:40]!r}) as the other side has it"
        else:
            continue
        out = []
        bad = check_parity(pb, po, pt, mutated, out)
        print(f"  self-test parity, {name}: {'found' if bad else 'MISSED'}")
        found += bool(bad)
        extra += 1
    # Train 6: every row both sides changed, as one side has it, and its
    # longest run a side wrote with two tokens swapped.
    for key in [k for k in m_rows if k[0] == "row"]:
        lo, lt, lb, row = o.get(key), t.get(key), b.get(key), m_rows[key]
        if lb is None or lo in (None, lb) or lt in (None, lb):
            continue
        for side_name, other in (("ours", lo), ("theirs", lt)):
            out = []
            bad = check_parity(pb, po, pt, pm.replace(row, other, 1), out)
            print(f"  self-test parity, a row both sides changed ({key[1][:40]!r}) as {side_name} has it: "
                  f"{'found' if bad else 'MISSED'}")
            found += bool(bad)
            extra += 1
        run = max(clause_runs(lb, lo) + clause_runs(lb, lt), key=len)
        words = row.split(" ")
        at = next(i for i in range(len(words)) if [w for w in words[i:i + len(run)]] == run)
        # Two tokens of the run swapped, its multiset unchanged.
        x, y = next((i, i + 1) for i in range(at + 1, at + len(run) - 1) if words[i] != words[i + 1])
        words[x], words[y] = words[y], words[x]
        out = []
        bad = check_parity(pb, po, pt, pm.replace(row, " ".join(words), 1), out)
        print(f"  self-test parity, a row both sides changed ({key[1][:40]!r}), two tokens of a side's "
              f"clause swapped: {'found' if bad else 'MISSED'}")
        found += bool(bad)
        extra += 1
        # Round 2: the clause moved whole, anchors and all, to the start of
        # the row's last cell (after its fourth `|`), so the row keeps its
        # key, its multiset and every clause's contiguity.
        words = row.split(" ")
        at = next(i for i in range(len(words)) if words[i:i + len(run)] == run)
        moved = words[:at] + words[at + len(run):]
        cell = [i for i, w in enumerate(moved) if w == "|"][3] + 1
        moved = moved[:cell] + run + moved[cell:]
        out = []
        bad = check_parity(pb, po, pt, pm.replace(row, " ".join(moved), 1), out)
        print(f"  self-test parity, a row both sides changed ({key[1][:40]!r}), a side's clause moved "
              f"whole with its anchors: {'found' if bad else 'MISSED'}")
        found += bool(bad)
        extra += 1
    # Round 2: the first row one side alone changed, two of its tokens swapped.
    for key in [k for k in m_rows if k[0] == "row"]:
        lo, lt, lb, row = o.get(key), t.get(key), b.get(key), m_rows[key]
        if lb is None or (lo != lb) == (lt != lb):
            continue
        words = row.split(" ")
        i = next((i for i in range(3, len(words) - 1) if words[i] != words[i + 1]), None)
        if i is None:
            continue
        words[i], words[i + 1] = words[i + 1], words[i]
        out = []
        bad = check_parity(pb, po, pt, pm.replace(row, " ".join(words), 1), out)
        print(f"  self-test parity, a row one side changed ({key[1][:40]!r}), two tokens swapped: "
              f"{'found' if bad else 'MISSED'}")
        found += bool(bad)
        extra += 1
        break
    for i in range(len(lines) - 1):
        k1, k2 = (next((k for k, v in m_rows.items() if v == lines[j]), None) for j in (i, i + 1))
        if not (k1 and k2 and k1[0] == k2[0] == "row"):
            continue
        added1 = b.get(k1) is None and ((o.get(k1) is None) != (t.get(k1) is None))
        changed2 = b.get(k2) is not None and ((o.get(k2) != b[k2]) != (t.get(k2) != b[k2]))
        if added1 and changed2 and (o.get(k1) is None) == (o.get(k2) != b[k2]):
            swapped = lines[:i] + [lines[i + 1], lines[i]] + lines[i + 2:]
            out = []
            bad = check_parity(pb, po, pt, "\n".join(swapped), out)
            print(f"  self-test parity, {k1[1][:40]!r} and {k2[1][:40]!r} swapped: {'found' if bad else 'MISSED'}")
            found += bool(bad)
            extra += 1
    return found, len(cases) + bool(added_ids) + bool(deleted_ids) + 1 + extra + cases7


def main():
    if sys.argv[2] == "--self-test":
        caught, cases = self_test(sys.argv[1], sys.argv[3])
        print(f"self-test {sys.argv[3][:8]}: {caught} of {cases} found")
        sys.exit(0 if caught == cases else 1)
    repo, merges = sys.argv[1], sys.argv[2:]
    total = 0
    for merge in merges:
        ours, theirs = git(repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
        base = git(repo, "merge-base", ours, theirs).strip()
        out = []
        bad = check_parity(*(show(repo, c, PARITY) for c in (base, ours, theirs, merge)), out)
        bad += check_ledger(*(show(repo, c, LEDGER) for c in (base, ours, theirs, merge)), out)
        print(f"merge {merge[:8]}: ours {ours[:8]}, theirs {theirs[:8]}, base {base[:8]} -- "
              f"{'OK' if not bad else str(bad) + ' DIFFERENCES'}")
        print("\n".join(out))
        total += bad
    sys.exit(1 if total else 0)


main()
