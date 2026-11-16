#!/usr/bin/env python3
"""Counts a starve.sh run of whole tui suites (`@suite`), per build label:
each case's verdicts -- the paced hostile paste, the two unpaced ones, the
status-frame test -- and why each red was red; then every hostile-paste
session, accepted or rejected, by its pauses (`0ns` the unpaced cases,
`12ms` the paced one), and each rejected one by where its pause fell (the
words tests/tui.rs's where_it_fell writes since 8ab62f69): across a planned
pause (and how long that sleep took), between parts with none planned, or
inside a part behind a full queue's poll (and how long that poll slept).

    python3 -I -B count_gaps.py <starve-out-dir>
Prints no path.
"""
import collections
import os
import re
import sys

CASES = {
    "paced": "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing",
    "unpaced, types nothing": "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing",
    "unpaced, no approval": "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval",
    "status frames": "the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out",
}
GAP = r"([\d.]+(?:ms|µs|ns|s))"


def ms(text):
    value, unit = re.match(r"([\d.]+)(ms|µs|us|ns|s)", text).groups()
    return float(value) * {"ms": 1, "µs": 1e-3, "us": 1e-3, "ns": 1e-6, "s": 1e3}[unit]


def kind_of(fell):
    if "across a planned" in fell:
        return "across a planned pause"
    if "with no pause planned there" in fell:
        return "between parts, none planned"
    if "inside part" in fell:
        return "inside a part, behind a poll"
    return "unsaid"


out = sys.argv[1]
accepted_where = collections.defaultdict(collections.Counter)
wrote = collections.defaultdict(list)
rest = collections.defaultdict(list)
verdicts = collections.defaultdict(collections.Counter)
reds = collections.defaultdict(list)
accepted = collections.defaultdict(list)
rejected = collections.defaultdict(list)
where = collections.defaultdict(collections.Counter)
slept = collections.defaultdict(list)
others = collections.defaultdict(collections.Counter)
suites = collections.Counter()
for log in sorted(os.listdir(out)):
    if not log.endswith(".log") or ".@suite." not in log:
        continue
    label = log.split(".")[0]
    suites[label] += 1
    text = open(os.path.join(out, log), errors="replace").read()
    for short, name in CASES.items():
        m = re.search(rf"test {name} \.\.\. (ok|FAILED)", text)
        verdict = m.group(1) if m else "not run"
        verdicts[label][(short, verdict)] += 1
        if verdict == "FAILED":
            after = text.split(f"---- {name} stdout ----", 1)[-1]
            said = re.search(r"panicked at [^\n]*\n([^\n]*)", after)
            line = said.group(1) if said else "no panic line"
            kind = "every session rejected" if "no session tested the paste" in line else line[:140]
            reds[label].append(f"{log}: {short}: {kind}")
    for failed in re.findall(r"test (\S+) \.\.\. FAILED", text):
        if failed not in CASES.values():
            others[label][failed] += 1
    for gap, pause, fell in re.findall(rf"the paste's longest pause: {GAP} \(pauses of (\S+)\)(?:, (.*?);$)?", text, re.M):
        accepted[(label, pause)].append(ms(gap))
        if fell:
            accepted_where[(label, pause)][kind_of(fell)] += 1
    for gap, pause, fell in re.findall(
            rf"a timing-indeterminate session: the paste paused for {GAP} \(pauses of (\S+)\), (.*?); a second later", text):
        rejected[(label, pause)].append(ms(gap))
        kind = kind_of(fell)
        s = re.search(rf"that slept {GAP}", fell)
        if s:
            slept[(label, pause, kind)].append(ms(s.group(1)))
        w = re.search(rf"the write that ended it took {GAP}, and the rest, {GAP},", fell)
        if w:
            wrote[(label, pause, kind)].append(ms(w.group(1)))
            rest[(label, pause, kind)].append(ms(w.group(2)))
        where[(label, pause)][kind] += 1
for label in sorted(suites):
    print(f"== {label}: {suites[label]} whole-suite runs")
    for short in CASES:
        cells = ", ".join(f"{v} {n}" for (s, v), n in sorted(verdicts[label].items()) if s == short)
        print(f"  {short}: {cells}")
    for red in reds[label]:
        print(f"    red: {red}")
    if others[label]:
        print("  other tests failed: " + ", ".join(f"{n} x{c}" for n, c in sorted(others[label].items())))
    for pause in sorted({p for (l, p) in set(accepted) | set(rejected) if l == label}):
        acc, rej = accepted[(label, pause)], rejected[(label, pause)]
        total = len(acc) + len(rej)
        line = f"  sessions with pauses of {pause}: {total}, rejected {len(rej)}"
        if total:
            line += f" ({100 * len(rej) / total:.1f}%)"
        if acc:
            acc.sort()
            line += f"; accepted longest: median {acc[len(acc) // 2]:.1f} ms, max {acc[-1]:.1f} ms"
        print(line)
        for kind, n in sorted(accepted_where[(label, pause)].items()):
            print(f"    accepted, its longest pause {kind}: {n}")
        for kind, n in sorted(where[(label, pause)].items()):
            s = sorted(slept[(label, pause, kind)])
            extra = f"; the sleep took {s[0]:.0f}-{s[-1]:.0f} ms, median {s[len(s) // 2]:.0f}" if s else ""
            w, r = sorted(wrote[(label, pause, kind)]), sorted(rest[(label, pause, kind)])
            if w:
                extra += (f"; the ending write {w[0]:.0f}-{w[-1]:.0f} ms, median {w[len(w) // 2]:.0f}"
                          f"; the rest {r[0]:.0f}-{r[-1]:.0f} ms, median {r[len(r) // 2]:.0f}")
            print(f"    rejected {kind}: {n}{extra}")
