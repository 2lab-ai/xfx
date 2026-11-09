#!/usr/bin/env python3
"""Train 11, round 3's look-ahead: every place under src/ and tests/ that
goes back in time from a boot-relative clock -- the catalog's (`CatalogTime`,
counted from boot) or `Instant` (`CLOCK_MONOTONIC` on Linux,
`CLOCK_UPTIME_RAW` on Apple, both from boot) -- so that a machine booted
less than that long ago has no such reading:

- `X.checked_sub(D)` where X is a clock reading (`Instant::now()`,
  `catalog_now()`, `<clock>.now()`, `.now()`), the statement read across its
  lines;
- `X - D` with D a `Duration` and X anything but a `Duration` constant: a
  reading, or a name bound to one in the function (`Instant - Duration`
  panics past the origin; `CatalogTime - Duration` saturates to it). Round
  3's first form took only `<call>) - D`, and missed 11 such sites, none more
  than a second back (`ci-young-clock.txt`). `net forward`: X adds a span to
  a reading first (`now + WINDOW - D`), or its name was bound to one
  (`Instant::now() + 3600 s`);
- `backdate_for_test(D)` / `backdate_catalog(.., D)` (`checked_sub(..)
  .unwrap_or(at)`: past the origin it leaves the time where it was);
- a call, with a literal duration, of a helper whose own site above takes
  its duration as a parameter (`catalog_due_ago(ago)`, `backdate_catalog(..,
  by)`): kind `via <helper>`.

Each with its file:line, whether it is in a tests module (or `#[cfg(test)]`),
the duration as written and in seconds where it is a literal, and the
enclosing function.

    python3 -I -B young_scan.py <repo-or-clone>
"""
import os
import re
import sys

root = sys.argv[1]
READING = re.compile(r"(Instant::now\(\)|catalog_now\(\)|clock\.now\(\)|\bnow\(\)|\.now\(\))")
DUR = re.compile(r"Duration::from_(secs|millis|micros|nanos)\((\d[\d_]*)(?:\s*\*\s*(\d[\d_]*))?\)|REFRESH_INTERVAL|failure_backoff\(\d+\)")
UNIT = {"secs": 1, "millis": 1e-3, "micros": 1e-6, "nanos": 1e-9}
MINUS = re.compile(r"\s-\s*(?:std::time::|crate::provider::model::)?(?:Duration::|REFRESH_INTERVAL)")


def seconds(text):
    total, found = 0.0, False
    for m in DUR.finditer(text):
        found = True
        if m.group(0) == "REFRESH_INTERVAL":
            total += 300
        elif m.group(0).startswith("failure_backoff"):
            total += 5
        else:
            n = int(m.group(2).replace("_", "")) * int((m.group(3) or "1").replace("_", ""))
            total += n * UNIT[m.group(1)]
    return total if found else None


rows = []
for base in ("src", "tests"):
    for dirpath, _, names in os.walk(os.path.join(root, base)):
        for name in names:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            lines = open(path, encoding="utf-8").read().split("\n")
            tests_at = next((i for i, l in enumerate(lines) if re.match(r"^\s*mod tests \{", l)), len(lines))
            fn, fn_at = None, 0
            for i, line in enumerate(lines):
                m = re.match(r"^\s*(?:pub(?:\(crate\)|\(super\))? )?(?:async )?fn ([a-z_0-9]+)", line)
                if m:
                    fn, fn_at = m.group(1), i
                code = line.split("//")[0]
                stmt = " ".join(l.split("//")[0].strip() for l in lines[max(0, i - 3):i + 1])
                kind = None
                # The receiver: what stands before this line's `.checked_sub(`,
                # back to the statement's start.
                receiver = stmt.rsplit(".checked_sub(", 1)[0].split(";")[-1]
                # Or a name bound to a reading in the dozen lines above.
                name_m = re.search(r"(?:let\s+\w+\s*=\s*)?\b([a-z_][a-z_0-9]*)\s*$", receiver.strip())
                bound = name_m and any(re.search(r"let\s+" + name_m.group(1) + r"\s*=.*" + READING.pattern, l)
                                       for l in lines[fn_at:i])
                if ".checked_sub(" in code and (READING.search(receiver) or bound):
                    kind = "checked_sub"
                    arg = code.split(".checked_sub(", 1)[1]
                elif MINUS.search(code) and not re.fullmatch(
                        r"\s*([A-Z][A-Z_0-9]*|(std::time::)?Duration::\w+\([^()]*\))\s*",
                        left := re.split(r"[(,{!]|=\s|\breturn\b", code[:MINUS.search(code).start()])[-1]):
                    # The left operand, back to its `(`, `,`, `=` or the
                    # line's start; a `Duration` constant alone is not a reading.
                    kind = "minus"
                    arg = code[MINUS.search(code).start():]
                    first = re.match(r"\s*([a-z_][a-z_0-9]*)", left)
                    if " + " in left or (first and any(
                            re.search(r"let\s+" + first.group(1) + r"\s*=.*\+", l) for l in lines[fn_at:i])):
                        kind = "minus net forward"
                elif re.search(r"backdate_(for_test|catalog)\(", code) and "fn backdate" not in code:
                    kind = "backdate"
                    arg = code
                if not kind:
                    continue
                cfg_test = i >= tests_at or any("#[cfg(test)]" in l for l in lines[max(0, i - 40):i] if l.strip().startswith("#[cfg(test)]") and False)
                in_tests = i >= tests_at
                s = seconds(arg if kind != "checked_sub" else arg + " " + " ".join(lines[i + 1:i + 2]))
                # The clock it goes back on: a FakeMachine made earlier in the
                # same function (whose readings start an hour in), else the
                # system's; and which reading: `Instant` or the catalog's.
                fake = any("FakeMachine::new()" in l for l in lines[fn_at:i])
                own = stmt.rsplit(".checked_sub(", 1)[0].split(";")[-1] if kind == "checked_sub" else (
                    left if kind.startswith("minus") else code)
                # The name the reading goes back from: a `checked_sub`'s
                # receiver's last, a minus's left operand's first.
                bound_name = (re.match(r"\s*([a-z_][a-z_0-9]*)", own) if kind.startswith("minus")
                              else re.search(r"\b([a-z_][a-z_0-9]*)\s*$", own.strip()))
                instant = "Instant::now()" in own or bool(bound_name and any(
                    re.search(r"let\s+" + bound_name.group(1) + r"\s*=.*Instant::now\(\)", l)
                    for l in lines[fn_at:i]))
                kind += (" fake" if fake else " system") + (" Instant" if instant else " catalog")
                rows.append((rel, i + 1, kind, "tests" if in_tests else "code", fn, s, code.strip()[:90]))
helpers = {r[4] for r in rows if r[5] is None and r[2].split()[0] in ("checked_sub", "backdate")}
for base in ("src", "tests"):
    for dirpath, _, names in os.walk(os.path.join(root, base)):
        for name in names:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            lines = open(path, encoding="utf-8").read().split("\n")
            tests_at = next((i for i, l in enumerate(lines) if re.match(r"^\s*mod tests \{", l)), len(lines))
            fn, fn_at = None, 0
            for i, line in enumerate(lines):
                m = re.match(r"^\s*(?:pub(?:\(crate\)|\(super\))? )?(?:async )?fn ([a-z_0-9]+)", line)
                if m:
                    fn, fn_at = m.group(1), i
                code = line.split("//")[0]
                fake = any("FakeMachine::new()" in l for l in lines[fn_at:i])
                for h in helpers:
                    if h and re.search(r"\b" + h + r"\(", code) and f"fn {h}(" not in code and h != "backdate_catalog" or (
                            h == "backdate_catalog" and "backdate_catalog(" in code and "fn backdate_catalog" not in code
                            and not any(r[0] == rel and r[1] == i + 1 for r in rows)):
                        rows.append((rel, i + 1, f"via {h}" + (" fake" if fake else " system") + " catalog",
                                     "tests" if i >= tests_at else "code", fn, seconds(code), code.strip()[:90]))
rows = sorted(set(rows))
for r in sorted(rows, key=lambda r: (r[3], -(r[5] or 0), r[0], r[1])):
    rel, n, kind, where, fn, s, text = r
    print(f"{rel}:{n}\t{where}\t{kind}\t{'' if s is None else f'{s:g} s'}\t{fn}\t{text}")
print(f"# {len(rows)} sites; outside tests modules: {sum(r[3] == 'code' for r in rows)}; "
      f"in tests going back more than 60 s on the system clock: "
      f"{sum(r[3] == 'tests' and (r[5] or 0) > 60 and ' system' in r[2] for r in rows)}; "
      f"more than 10 s on it: {sum(r[3] == 'tests' and (r[5] or 0) > 10 and ' system' in r[2] for r in rows)}")
