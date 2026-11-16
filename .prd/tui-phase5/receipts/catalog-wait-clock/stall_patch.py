#!/usr/bin/env python3
"""Plants the scratch stall points in a throwaway tree (never the worktree):

    python3 -I -B stall_patch.py <tree>

Each point sleeps the thread it runs on for the milliseconds its variable
names, when set -- a machine holding that thread there:

- TURN   `XFX_SCRATCH_TURN_STALL_MS`: the loop's thread, right after each
  turn sends `TurnStarted` (`run_turn`). Round 1's only point.
- SELECT `XFX_SCRATCH_SELECT_STALL_MS`: the loop's thread, before the
  loop's first select of each `turn_loop` (where a stop read before the
  first prompt is taken drops it, `Taken::Abandon`).
- START  `XFX_SCRATCH_START_STALL_MS`: the test's thread, right before the
  harness reads its wall-clock start -- `first_of_two_and_a_stop`'s (round
  3: `Held::run`'s) and the twenty-question test's (at the base after the
  load's opener has started, at the head before it).

Every anchor must match exactly the expected number of times, or nothing is
written. Base and head differ only in the START anchors, both listed.
"""
import sys

W = "src/tui/worker.rs"


def stall(var, indent):
    pad = " " * indent
    return (f"{pad}// SCRATCH ONLY (catalog-wait-clock RED/GREEN): {var}.\n"
            f"{pad}if let Some(ms) = std::env::var(\"{var}\").ok().and_then(|v| v.parse::<u64>().ok()) {{\n"
            f"{pad}    std::thread::sleep(std::time::Duration::from_millis(ms));\n"
            f"{pad}}}\n")


def main():
    path = f"{sys.argv[1]}/{W}"
    text = open(path, encoding="utf-8").read()
    turn = "    let _ = bridge::send_ui(events, &turn.token, started).await;\n"
    assert text.count(turn) == 1, ("TURN", text.count(turn))
    text = text.replace(turn, turn + stall("XFX_SCRATCH_TURN_STALL_MS", 4))
    select = ("    let mut changes_open = true;\n    loop {\n")
    assert text.count(select) == 1, ("SELECT", text.count(select))
    text = text.replace(select, "    let mut changes_open = true;\n"
                        + stall("XFX_SCRATCH_SELECT_STALL_MS", 4) + "    loop {\n")
    # START: the harness's start read, base form or head form.
    planted = 0
    for anchor, indent in [
        ("        let outstanding = AtomicUsize::new(2);\n        let started = Instant::now();\n", 8),
        ("        let outstanding = AtomicUsize::new(2);\n        // The wall clock's count begins before a first load's opener does:\n", 8),
        # Round 3's head: the start is read in `Held::run`, before any opener.
        ("        ) -> T {\n            let begun = Instant::now();\n", 8),
        ("            let outstanding = AtomicUsize::new(1);\n            let started = Instant::now();\n", 12),
        ("            let outstanding = AtomicUsize::new(1);\n            // Before the opener: a thread held between them cannot make the\n", 12),
    ]:
        n = text.count(anchor)
        assert n <= 1, ("START", anchor, n)
        if n:
            first, rest = anchor.split("\n", 1)
            pad = 12 if first.strip() == ") -> T {" else indent
            text = text.replace(anchor, first + "\n" + stall("XFX_SCRATCH_START_STALL_MS", pad) + rest)
            planted += 1
    assert planted == 2, ("START planted", planted)
    open(path, "w", encoding="utf-8").write(text)
    print("planted TURN, SELECT, START x2")


main()
