#!/usr/bin/env python3
"""Train 11, round 4: the look-ahead's superset at c4354ec7 -- round 3's
pattern plus every binary minus of a name or constant, over `.rs` files under
src/ and tests/; the lines that are not young_scan.py's sites and name a time,
each in the class it was read into (the table below is that reading).

    python3 -I -B young_superset.py <git archive of c4354ec7> young-scan-c4354ec7.txt
"""
import os, re, sys
root, scan = sys.argv[1], sys.argv[2]
P = (r'\.checked_sub\(|\.saturating_sub\(|\.(checked_)?duration_since\(|\s-\s*(std::time::)?Duration::|-=|'
     r'backdate_(for_test|catalog)\(|catalog_due_ago\(|\s-\s+[A-Za-z_]')
T = (r'[Dd]uration|Instant|now|elapsed|deadline|clock|since|time|secs|millis|_at\b|\bat\b|began|due|catalog|ago\b|'
     r'until|expires|answered|not_before|quiet|t0')
sites = {l.split("\t")[0] for l in open(scan) if not l.startswith("#")}
sup = []
for base in ("src", "tests"):
    for d, _, names in os.walk(os.path.join(root, base)):
        for n in sorted(names):
            if not n.endswith(".rs"):
                continue
            path = os.path.join(d, n)
            rel = os.path.relpath(path, root)
            for i, line in enumerate(open(path, encoding="utf-8").read().split("\n"), 1):
                if re.match(r"\s*//", line):
                    continue
                if re.search(P, line):
                    sup.append((f"{rel}:{i}", line.strip()))
rest = [s for s in sup if s[0] not in sites]
timed = [s for s in rest if re.search(T, s[0] + ":" + s[1])]  # the path too, as grep -rn printed it (round 3)
cls = {
 "the definitions (the scan lists their callers)": ["src/tui/worker.rs:17177", "src/tui/event_loop.rs:2589", "src/provider/model.rs:2397", "src/provider/model.rs:2400", "src/provider/catalog_clock.rs:35", "src/provider/catalog_clock.rs:41", "src/provider/catalog_clock.rs:57"],
 "FakeMachine's own bookkeeping, or a forward reading in a test": ["src/provider/catalog_clock.rs:189", "src/provider/catalog_clock.rs:190", "src/provider/catalog_clock.rs:315"],
 "a span left to wait, or Duration constants (no reading goes back)": ["src/process.rs:341", "src/tui/permissions.rs:428", "src/tui/probe.rs:474", "src/tui/gesture.rs:377"],
 "a difference of two readings (it measures; nothing goes back): `at - t0` on Instants, a thread CPU time": ["src/tui/input.rs:3881", "src/tui/input.rs:3886", "src/markdown/tests/mod.rs:102"],
 "the wall clock (UNIX_EPOCH, *_ms, an i64 of ms), not boot-relative": ["src/tui/full_transcript.rs:372", "src/tui/usage_menu.rs:500", "src/tui/session_picker.rs:957", "src/usage/clock.rs:121", "src/usage/cache/tests.rs:1390", "src/usage/policy.rs:59", "src/mcp/docker.rs:110", "src/skills/install.rs:363", "src/permission/session_rules.rs:791", "src/session/event.rs:694", "src/session/event.rs:716", "src/permission/authority.rs:128", "tests/tui.rs:5988", "src/usage/policy.rs:207", "src/usage/cache/tests.rs:460"],
}
where = {x: k for k, v in cls.items() for x in v}
unused = [x for x in where if x not in {s[0] for s in timed}]
assert not unused, unused
print("# The look-ahead's superset at c4354ec7 (git archive of the commit; round 4: round 3's pattern plus every binary minus of a name or constant, `.rs` files only): every line under src/ and tests/ matching")
print("#   " + P)
print("# outside a `//` comment; then those that are not young-scan-c4354ec7.txt's sites and name a time (" + T + ") in the path or the line, each with its class, read")
print(f"# {len(sup)} lines; {len(sup) - len(rest)} of them the scan's {len(sites)} sites; {len(rest)} others, {len(timed)} naming a time (below); the {len(rest) - len(timed)} naming none taken as integer arithmetic")
groups = {}
for loc, text in timed:
    groups.setdefault(where.get(loc, "integer arithmetic on rows, columns, bytes, indices or counts (a name such as `at`, `now`, `start`)"), []).append(f"{loc}:{text}"[:150])
for k in sorted(groups, key=lambda k: -len(groups[k])):
    print(f"## {k}: {len(groups[k])}")
    for l in groups[k]:
        print("    " + l)
