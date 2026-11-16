#!/usr/bin/env python3
"""The join review's SHOULD 2, pinned RED: orders of the production pass
(`loop_turn`, src/tui/event_loop.rs) broken one at a time on a throwaway
clone -- never the worktree -- and the three join cases run against each:

  settled  -- the pair settled in the tick that reads the start, through
              the loop's turn (9a54c911);
  by-hand  -- the pair still pending there, the turn's steps called by hand
              (9a54c911's second branch, its own case since 25203a2c);
  loop     -- the pending pair through the loop's own turns (25203a2c).

The baseline runs first, unmutated. Each run is bounded (TIMEOUT, its
build included). Prints, per order, each case's verdict and, for a failed
one, its panic's message line. Prints no path.

    python3 -I -B order_red.py <clone> <cargo-target-dir>
"""
import os
import re
import signal
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target
PATH = "src/tui/event_loop.rs"
TIMEOUT = 1200
CASES = {
    "settled": "tui::event_loop::tests::an_escape_pair_settled_in_the_tick_that_reads_a_held_prompts_start_leaves_the_start_ending_the_drain",
    "by-hand": "tui::event_loop::tests::an_escape_pair_still_pending_in_the_tick_that_reads_a_held_prompts_start_is_no_key_and_the_start_ends_the_drain",
    "loop": "tui::event_loop::tests::an_escape_pair_pending_through_one_loop_turn_settles_in_the_next_ahead_of_the_start_it_reads",
}
READ = "    read(shell)?;\n"
SETTLE = "    shell.settle_input(Instant::now());\n"
DRAIN = ("    // What the runtime produced since the last turn. Text, not terminal\n"
         "    // state, and therefore outside the reconciles for the same reason.\n"
         "    take_ui_events(shell, events);\n")
BAND = "    shell.settle_band(Instant::now());\n    let reconciled = reconcile(shell, band)?;\n"
BAND_OUT = "    let reconciled = reconcile(shell, band)?;\n"
# (name, [(old, new), ...]) -- each old must stand once in the file.
ORDERS = [
    ("baseline", []),
    # The reviewer's example: the band settled before the input.
    ("band-before-input", [(BAND, BAND_OUT), (SETTLE + DRAIN, "    shell.settle_band(Instant::now());\n" + SETTLE + DRAIN)]),
    # The input settled after the drain (mutants.py's loop-settles-input-after-the-drain).
    ("input-after-drain", [(SETTLE + DRAIN, DRAIN + SETTLE)]),
    # The drain before the read, so before the input is settled too.
    ("drain-before-read", [(SETTLE + DRAIN, SETTLE), ("    read(shell)?;\n    // What the bytes", "    take_ui_events(shell, events);\n" + READ + "    // What the bytes")]),
]


def bounded(command):
    proc = subprocess.Popen(command, cwd=clone, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=TIMEOUT)
        return proc.returncode, out, err
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        return None, out, err


file = os.path.join(clone, PATH)
bad = 0
for name, edits in ORDERS:
    text = open(file).read()
    for old, new in edits:
        if text.count(old) != 1:
            print(f"{name}: NOT APPLIED ({text.count(old)} matches of {old.splitlines()[0]!r})")
            bad += 1
            break
        text = text.replace(old, new)
    else:
        open(file, "w").write(text)
        try:
            code, out, err = bounded(["cargo", "test", "--locked", "--lib", "--", "--exact"] + list(CASES.values()))
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", PATH], env=env, check=True)
        if code is None:
            print(f"{name}: TIMED OUT after {TIMEOUT} s")
            bad += 1
            continue
        cells = []
        for short, full in CASES.items():
            m = re.search(r"^test " + re.escape(full) + r" \.\.\. (\w+)$", out, re.M)
            verdict = m.group(1) if m else "MISSING"
            if verdict == "FAILED":
                after = out.split(f"---- {full} stdout ----", 1)[-1].splitlines()
                at = next((i for i, l in enumerate(after) if " panicked at " in l), None)
                where = after[at].split(" panicked at ", 1)[1].rstrip(":") if at is not None else "?"
                why = after[at + 1].strip() if at is not None and at + 1 < len(after) else ""
                verdict += f" at {where}: {why[:110]}"
            cells.append(f"{short} {verdict}")
        print(f"{name}: " + "; ".join(cells))
sys.exit(1 if bad else 0)
