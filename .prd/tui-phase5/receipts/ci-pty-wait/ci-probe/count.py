"""Reads red.sh's logs back by their words: per recipe and build, each
case's runs green and red, and its sessions -- rejected with CI's signature
(across a planned pause that slept 50 ms or more, the write that ended it
under a millisecond), rejected otherwise, accepted -- with the spread of
the accepted sessions' longest pauses, of the planned pauses' sleeps, and
of the full-queue polls that ended a session's longest pause. A red run
whose failure is not unpaced_case's "pastes paused" is an accepted session
that failed an assertion, and is counted apart.

    python3 -I count.py <out-dir>
"""
import collections
import os
import re
import sys

out = sys.argv[1]
ms = lambda v, u: float(v) * {"ms": 1, "µs": 1e-3, "ns": 1e-6, "s": 1e3}[u]
DUR = r"([\d.]+)(ms|µs|ns|s)"
CASES = {
    "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing": "paced",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing": "unpaced-types",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval": "unpaced-approval",
}


def spread(values):
    if not values:
        return "none"
    values = sorted(values)
    return f"{values[0]:.1f}-{values[-1]:.1f} ms, median {values[len(values) // 2]:.1f} (n {len(values)})"


groups = collections.defaultdict(list)
for name in sorted(os.listdir(out)):
    m = re.fullmatch(r"(.+)\.(.+)\.(\d+)\.log", name)
    if m:
        groups[(m.group(1), m.group(2))].append(name)
for (recipe, build), names in sorted(groups.items()):
    runs = collections.Counter()
    green = collections.Counter()
    asserted = collections.Counter()
    sessions = collections.Counter()
    longest = collections.defaultdict(list)
    planned, polls, seconds = [], [], []
    for name in names:
        text = open(os.path.join(out, name), errors="replace").read()
        seconds += [int(s) for s in re.findall(r"^seconds (\d+)$", text, re.M)]
        for case, short in CASES.items():
            if re.search(rf"^test {case} \.\.\. (ok|FAILED)$", text, re.M):
                runs[short] += 1
                if re.search(rf"^test {case} \.\.\. ok$", text, re.M):
                    green[short] += 1
        # A red case whose panic is not unpaced_case's: an accepted session failed.
        for panic in re.findall(r"panicked at [^\n]*\n([^\n]*)", text):
            if "pastes paused for" not in panic:
                asserted["any"] += 1
        for line in re.findall(r"(?:the paste's longest pause|a timing-indeterminate session)[^\n]*", text):
            kind = "unpaced" if "(pauses of 0ns)" in line else "paced"
            gap = re.search(r"(?:longest pause: |paused for )" + DUR, line)
            slept = re.search(r"planned \d+ms pause that slept " + DUR, line)
            poll = re.search(r"2ms poll that slept " + DUR, line)
            wrote = re.search(r"the write that ended it took " + DUR, line)
            if slept:
                planned.append(ms(*slept.groups()))
            if poll:
                polls.append(ms(*poll.groups()))
            if "timing-indeterminate" in line:
                signature = slept and ms(*slept.groups()) >= 50 and wrote and ms(*wrote.groups()) < 1
                sessions[(kind, "rejected, CI's signature" if signature else "rejected otherwise")] += 1
            else:
                sessions[(kind, "accepted")] += 1
                longest[kind].append(ms(*gap.groups()))
    print(f"## {recipe}, {build}: {len(names)} runs, seconds per run {spread(seconds).replace(' ms', ' s')}")
    for short in CASES.values():
        print(f"  {short}: green {green[short]} / {runs[short]}")
    print(f"  red runs failed by an accepted session's assertion: {asserted['any']}")
    for kind in ("paced", "unpaced"):
        counts = ", ".join(f"{what} {sessions[(kind, what)]}" for what in
                           ("accepted", "rejected, CI's signature", "rejected otherwise"))
        print(f"  {kind} sessions: {counts}; accepted longest pause {spread(longest[kind])}")
    print(f"  planned pauses that ended a session's longest pause slept {spread(planned)}")
    print(f"  2 ms polls that ended a session's longest pause slept {spread(polls)}")
