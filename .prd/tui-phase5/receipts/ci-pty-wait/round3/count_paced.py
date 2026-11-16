#!/usr/bin/env python3
"""Counts a starve.sh run's hostile-paste sessions, by the pause each case
takes between its parts (printed by tests/tui.rs's paste_with_pauses).

    python3 -I count_paced.py <starve-out-dir>

For each pause (`0ns` = the unpaced cases, `20ms` = the paced one): the
sessions accepted (with their longest pauses), the sessions rejected as
timing-indeterminate, and for the paced case's runs: green, red with every
session rejected ("no session tested the paste"), and red on an accepted
session (every pause under 50 ms). Also every other test that failed in a
suite run. Prints no path.
"""
import collections
import os
import re
import sys

PACED = "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing"
out = sys.argv[1]
# Keyed by (where, pause): `single` logs hold one test's output whole;
# `suite` logs interleave a parallel suite's, so a diagnostic line there can
# be cut by another test's and go uncounted (the `cut` line below).
accepted = collections.defaultdict(list)
rejected = collections.defaultdict(list)
runs = collections.Counter()
reds = []
others = collections.Counter()


def ms(text):
    value, unit = re.match(r"([\d.]+)(ms|µs|us|ns|s)", text).groups()
    return float(value) * {"ms": 1, "µs": 1e-3, "us": 1e-3, "ns": 1e-6, "s": 1e3}[unit]


for log in sorted(os.listdir(out)):
    if not log.endswith(".log"):
        continue
    label, name = log.split(".")[:2]
    text = open(os.path.join(out, log), errors="replace").read()
    source = "suite" if name == "@suite" else "single"
    # `(parts X apart)` at 6acbcd7e, a pause before every part; `(pauses of
    # X)` since, a pause before four.
    for gap, *pause in re.findall(
            r"the paste's longest pause: (\S+) \((?:parts (\S+) apart|pauses of (\S+))\)", text):
        accepted[(source, pause[0] or pause[1])].append(ms(gap))
    for gap, *pause in re.findall(
            r"a timing-indeterminate session: the paste paused for (\S+)\s+"
            r"\((?:parts (\S+) apart|pauses of (\S+))\)", text):
        rejected[(source, pause[0] or pause[1])].append(ms(gap))
    if name == PACED:
        verdict = "ok" if "test result: ok." in text else "FAILED"
    elif name == "@suite":
        # Not anchored: with --nocapture another test's line can be cut
        # around this one.
        m = re.search(rf"test {PACED} \.\.\. (ok|FAILED)", text)
        verdict = m.group(1) if m else "not run"
        for failed in re.findall(r"^test (\S+) \.\.\. FAILED", text, re.M):
            if failed != PACED:
                others[failed] += 1
    else:
        continue
    where = "single" if name == PACED else "suite"
    if verdict == "ok":
        runs[f"{where} green"] += 1
        continue
    panic = re.search(rf"thread '{PACED}' \(\d+\) panicked at ([^\n]+)\n([^\n]*)", text)
    message = panic.group(2) if panic else "no panic line"
    if "no session tested the paste" in message:
        runs[f"{where} red, every session rejected"] += 1
    else:
        runs[f"{where} red on an accepted session (every pause under 50 ms)"] += 1
    reds.append(f"{log}: {panic.group(1) if panic else ''} {message[:160]}")

print("paced case runs (single = its own process, suite = inside a whole parallel tui suite):")
for key, n in sorted(runs.items()):
    print(f"  {key}: {n}")
for source, pause in sorted(set(accepted) | set(rejected)):
    acc, rej = accepted[(source, pause)], rejected[(source, pause)]
    total = len(acc) + len(rej)
    line = (f"{source} runs, sessions with pauses of {pause}: {total}, "
            f"accepted {len(acc)}, rejected {len(rej)}")
    if total:
        line += f" ({100 * len(rej) / total:.1f}% rejected)"
    if acc:
        acc.sort()
        line += f"; accepted longest pauses: min {acc[0]:.1f} ms, median {acc[len(acc) // 2]:.1f} ms, max {acc[-1]:.1f} ms"
    if rej:
        line += f"; rejected pauses: {', '.join(f'{g:.0f} ms' for g in sorted(rej))}"
    print(line)
cut = 0
for log in sorted(os.listdir(out)):
    if log.endswith(".log"):
        text = open(os.path.join(out, log), errors="replace").read()
        cut += len(re.findall(r"the paste's longest pause: |a timing-indeterminate session: ", text)) - len(
            re.findall(r"(the paste's longest pause: \S+ |a timing-indeterminate session: the paste paused for \S+\s+)"
                       r"\((?:parts \S+ apart|pauses of \S+)\)", text))
print(f"diagnostic lines another test's output cut (not counted above): {cut}")
print("paced reds:" if reds else "paced reds: none")
for red in reds:
    print("  " + red)
print("other tests failed in suite runs:" if others else "other tests failed in suite runs: none")
for name, n in sorted(others.items()):
    print(f"  {name}: {n}")
