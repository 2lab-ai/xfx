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

Prints one line per difference from the expectation, then a summary per
merge; exit 1 if any.

    python3 -I -B token_check.py <repo> --self-test <merge>

The negative control: the merge's own texts with one token a side added
taken out of its row, one token a side deleted put back, one ledger entry a
side added dropped, one entry a side deleted restored, and one comment line
dropped -- each must be found.
"""
import collections
import re
import subprocess
import sys
import tomllib

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
        else:
            added = sum(((co - cb) + (ct - cb)).values())
            deleted = sum(((cb - co) + (cb - ct)).values())
            out.append(f"  parity row {label!r}: changed by {'+'.join(sides) or 'none'}; "
                       f"{added} tokens added, {deleted} deleted, all as expected")
        changed.append((key, sides))
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
    return found, len(cases) + bool(added_ids) + bool(deleted_ids) + 1


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
