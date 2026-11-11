#!/usr/bin/env python3
"""The failure table and the timing comparison, as markdown, from table.json."""
import json, statistics, sys
t = json.load(open(sys.argv[1]))
WHAT = {
    "the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out@tests/tui.rs:852": "assert: reports were [idle, done], no `working`",
    "the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out@tests/tui.rs:857": "assert: `working` behind a frame showing `Generating`; no `Thinking` frame",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing@tests/support/pty.rs:409": "20 s wait for `┃ xz`: the paste body was typed as keys",
    "slash_skills_lists_installs_from_a_local_directory_and_never_from_the_network@tests/support/pty.rs:409": "20 s wait for `Skills 1`: `$foo` listed all 3 skills",
    "ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes@tests/support/pty.rs:367": "20 s wait: `Compaction cancelled.` still on screen behind MARKER-THREE",
}
print("| run / attempt | job | head | failing step | test | what failed | tests/tui.rs run (plain / fault injector) |")
print("|---|---|---|---|---|---|---|")
for r in t:
    if r["conclusion"] != "failure":
        continue
    plain = f"{r['tui_plain'][2]:.1f} s" if r["tui_plain"] else "-"
    fault = f"{r['tui_fault'][2]:.1f} s" if r["tui_fault"] else "not run"
    steps = ", ".join(r["failed_step"])
    if not r["failed"]:
        print(f"| {r['run']} / {r['attempt']} | {r['job']} | {r['sha']} | {steps} | - | not a PTY test (local-paths `shapes`, PR #95's class) | {plain} / {fault} |")
    for name, at in r["failed"]:
        print(f"| {r['run']} / {r['attempt']} | {r['job']} | {r['sha']} | {steps} | `{name}` | {WHAT.get(name + '@' + at, at)} | {plain} / {fault} |")
ok = [r for r in t if r["conclusion"] == "success"]
def stats(xs):
    xs = [x for x in xs if x is not None]
    return f"n={len(xs)}, min {min(xs):.1f}, median {statistics.median(xs):.1f}, max {max(xs):.1f}"
print()
print("Passing jobs' tests/tui.rs run time, seconds, by suite size (the suite grew 175 -> 189 cases and slowed on 2026-11-09):")
for lo, hi in ((0, 187), (188, 999)):
    sel = [r for r in ok if r["tui_plain"] and lo <= r["tui_plain"][1] <= hi]
    print(f"- {lo}-{hi if hi < 999 else '...'} plain cases: plain {stats([r['tui_plain'][2] for r in sel])}; fault injector {stats([r['tui_fault'][2] for r in sel if r['tui_fault']])}")
bad = [r for r in t if r["conclusion"] == "failure" and r["failed"]]
print(f"- failing jobs (PTY test failed): plain {stats([r['tui_plain'][2] for r in bad if r['tui_plain']])}; fault injector {stats([r['tui_fault'][2] for r in bad if r['tui_fault']])}")
print(f"- jobs read: {len(t)} completed aarch64-apple-darwin gate jobs, {len(ok)} passing")
