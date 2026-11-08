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
  contiguous run: a clause a side wrote is not split or reordered, and it
  stands where that side put it. The multiset
  check alone does not look at the order of tokens inside a row, and merges
  2 and 3 of this train each resolved two such rows word by word.

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
rule) -- each must be found.
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


def contiguous(run, row):
    n = len(run)
    return any(row[i:i + n] == run for i in range(len(row) - n + 1))


def check_parity(base, ours, theirs, merged, out):
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
                           f"side's token either side) whole and in place")
        else:
            added = sum(((co - cb) + (ct - cb)).values())
            deleted = sum(((cb - co) + (cb - ct)).values())
            out.append(f"  parity row {label!r}: changed by {'+'.join(sides) or 'none'}; "
                       f"{added} tokens added, {deleted} deleted, all as expected")
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
        elif em is not None and masked(eo) == masked(et) == masked(em):
            out.append(f"  ledger {key[0]} {key[1]}: both sides changed its path:line numbers alone; "
                       f"kept, the numbers re-pointed (resolve_refs.py, repointed.py)")
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


def self_test(repo, merge):
    ours, theirs = git(repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
    base = git(repo, "merge-base", ours, theirs).strip()
    pb, po, pt, pm = (show(repo, c, PARITY) for c in (base, ours, theirs, merge))
    lb, lo, lt, lm = (show(repo, c, LEDGER) for c in (base, ours, theirs, merge))
    found = 0
    # A row a side changed: one of the tokens it added taken out, then one
    # it deleted put back.
    b, o, t = rows(pb), rows(po), rows(pt)
    key = next(k for k in b if k[0] == "row" and (o.get(k) != b[k] or t.get(k) != b[k]))
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
    return found, len(cases) + bool(added_ids) + bool(deleted_ids) + 1 + extra


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
