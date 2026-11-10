#!/usr/bin/env python3
"""CATALOG-WAIT-CLOCK's mutants, each on a throwaway `git clone --no-local`,
never on the worktree; run over the base's clone and the head's, so each
row says whether the old tests and the new ones kill it.

    python3 -I -B mutants.py <clone> <label> <out> [<mutant>...]

For each mutant: the clone's tracked files are reset (`git checkout -- .`),
its replacements applied (each must match exactly once, or the mutant is
refused), the lib tests built, and the three catalog-wait tests run under a
LIMIT-second bound (their process group killed past it). `killed` when the
run fails, `SURVIVED` when it passes, `timeout` past the bound. The clone is
reset after each. Writes one line per mutant, the failing tests and their
assertion's `left`/`right` lines under it.
"""
import os
import re
import signal
import subprocess
import sys

LIMIT = 900
TESTS = [
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number",
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one",
    "tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop",
    "tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs",
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn",
    # Round 4's two: the watcher's unwind guard, and its late stamp.
    "tui::worker::tests::the_place_watcher_ends_when_the_run_it_watches_panics",
    "tui::worker::tests::a_late_watcher_moves_the_stop_to_end_reading_and_fails_it_only_past_the_bound",
]
SHORT = {TESTS[0]: "A", TESTS[1]: "B", TESTS[2]: "C", TESTS[3]: "D", TESTS[4]: "E",
         TESTS[5]: "F", TESTS[6]: "G"}
# Run over the whole lib suite, not the five: whether anything in the crate
# kills them (opus SHOULD 1).
FULL_LIB = {"window-first-load-ignores-stop", "image-route-read-ignores-stop"}

W = "src/tui/worker.rs"
M = "src/agent/machine.rs"
B = "src/tui/bridge.rs"
Q = "src/tools/question.rs"

# STOP-IN-CATALOG-WAIT's mutants.py, unchanged texts.
ARM = (
    "        // Stopped: nothing will be sent, so nothing is asked for, and the\n"
    "        // machine ends the turn as the stop (above).\n"
    "        Err(_) if turn.token.is_cancelled() => crate::gateway::protocol::RequestOptions::default(),\n"
)
SKIP = "    if carries_images && !turn.token.is_cancelled() {\n"
GUARD = (
    "        if self.request.cancel.is_cancelled() {\n"
    "            stopped = Some(TurnError::Cancelled);\n"
    "        } else if self.compactor.is_some() {\n"
)
EFFORT_END = (
    "            let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;\n"
    "        }\n"
    "    }\n"
    "    // **A stop that came while the catalog was waited for is the turn's end,\n"
)
FIRST_LOAD = (
    "    if !images.is_empty()\n"
    "        || !state.settings.effort.is_default()\n"
)

FIRST_LOAD_LATE = ("if !fresh && turn.token.is_cancelled() && state.selector.is_loading() "
                   "&& matches!(state.selector.catalog(), CatalogState::NotLoaded) {\n")
TOKIO_700 = "        tokio::time::sleep(std::time::Duration::from_millis(700)).await;"
THREAD_700 = "        std::thread::sleep(std::time::Duration::from_millis(700));"


def THREAD(ms):
    return f"        std::thread::sleep(std::time::Duration::from_millis({ms}));"


def XTHREAD(ms):
    # An answer from another OS thread `ms` later, awaited: the runtime parks
    # with no timer of its own standing.
    return ("        let (late_tx, late_rx) = tokio::sync::oneshot::channel::<()>();\n"
            "        std::thread::spawn(move || {\n"
            f"            std::thread::sleep(std::time::Duration::from_millis({ms}));\n"
            "            let _ = late_tx.send(());\n"
            "        });\n"
            "        let _ = late_rx.await;")

MUTANTS = {
    # The stop ignored until each wait ends by itself: its arm gone from the
    # select (train 7's J2 for the retry).
    "retry-wait-ignores-stop": [(
        W,
        "        () = turn.token.cancelled() => {}\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
    )],
    "window-first-load-ignores-stop": [(
        W,
        "        return;\n    }\n    let fresh = tokio::select! {\n        biased;\n"
        "        () = turn.token.cancelled() => false,\n",
        "        return;\n    }\n    let fresh = tokio::select! {\n        biased;\n",
    )],
    "turn-catalog-read-ignores-stop": [(
        W,
        "            () = turn.token.cancelled() => false,\n"
        "            fresh = state.load_catalog() => fresh,\n",
        "            fresh = state.load_catalog() => fresh,\n",
    )],
    # The stop honoured, half a second late: still inside the retry's 2 s,
    # and before a first load lands.
    "retry-wait-stop-late": [(
        W,
        "        () = turn.token.cancelled() => {}\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
        "        () = turn.token.cancelled() => tokio::time::sleep(std::time::Duration::from_millis(500)).await,\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
    )],
    "turn-catalog-read-stop-late": [(
        W,
        "            () = turn.token.cancelled() => false,\n"
        "            fresh = state.load_catalog() => fresh,\n",
        "            () = turn.token.cancelled() => {\n"
        "                tokio::time::sleep(std::time::Duration::from_millis(500)).await;\n"
        "                false\n"
        "            }\n"
        "            fresh = state.load_catalog() => fresh,\n",
    )],
    "image-route-read-ignores-stop": [(
        W,
        "        () = turn.token.cancelled() => false,\n"
        "        fresh = state.load_catalog() => fresh,\n    };\n    // The image route",
        "        fresh = state.load_catalog() => fresh,\n    };\n    // The image route",
    )],
    # STOP-IN-CATALOG-WAIT's own.
    "prompt-images-first-load-not-waited": [
        (W, FIRST_LOAD, "    if !state.settings.effort.is_default()\n")
    ],
    "ultra-refusal-not-yielded": [(W, ARM, "")],
    "image-refusal-asked-when-stopped": [(W, SKIP, "    if carries_images {\n")],
    "candidate-bare-return": [
        (W, ARM, ""),
        (W, SKIP, "    if carries_images {\n"),
        (
            W,
            EFFORT_END,
            EFFORT_END.replace(
                "    }\n    // **A stop",
                "    }\n    if turn.token.is_cancelled() {\n"
                "        return Some(bridge::TURN_CANCELLED.to_string());\n"
                "    }\n    // **A stop",
            ),
        ),
    ],
    "no-machine-guard": [(M, GUARD, "        if self.compactor.is_some() {\n")],
    "stop-leaks-to-next-turn": [(B, "        self.mirror.reset();\n", "")],
    # Round 2: the stop honoured 700 ms late, only while a first load is on
    # its way (the catalog not read yet and a load pending), at each of the
    # three first-load selects; once on a tokio timer, once by holding the
    # thread.
    **{f"first-load-late-{where}-{how}": [(W, anchor, anchor + indent + FIRST_LOAD_LATE + delay + "\n" + indent + "}\n")]
       for where, anchor, indent in [
           ("window", "        return;\n    }\n    let fresh = tokio::select! {\n        biased;\n"
                      "        () = turn.token.cancelled() => false,\n        fresh = state.load_catalog() => fresh,\n    };\n", "    "),
           ("image-route", "        () = turn.token.cancelled() => false,\n        fresh = state.load_catalog() => fresh,\n"
                           "    };\n    // The image route", None),
           ("turn-read", "            () = turn.token.cancelled() => false,\n            fresh = state.load_catalog() => fresh,\n"
                         "        };\n", "        "),
       ] if indent is not None
       for how, delay in [("tokio", TOKIO_700), ("thread", THREAD_700)]},
    **{f"first-load-late-image-route-{how}": [(
        W,
        "        () = turn.token.cancelled() => false,\n        fresh = state.load_catalog() => fresh,\n    };\n    // The image route",
        "        () = turn.token.cancelled() => false,\n        fresh = state.load_catalog() => fresh,\n    };\n"
        "    " + FIRST_LOAD_LATE + delay + "\n    }\n    // The image route",
    )] for how, delay in [("tokio", TOKIO_700), ("thread", THREAD_700)]},
    # Round 3: lateness off the loop runtime's timers, at the turn's read
    # (first loads only) and at the retry wait -- the thread held, and (gpt6)
    # a wait on a oneshot another OS thread answers, with no timer standing --
    # below the harness's wall bound (STOP_TO_END, 10 s from 9d2e7f3c; 5 s at
    # ce221384) and above it.
    **{f"first-load-late-turn-read-{how}-{ms}": [(
        W,
        "            () = turn.token.cancelled() => false,\n            fresh = state.load_catalog() => fresh,\n        };\n",
        "            () = turn.token.cancelled() => false,\n            fresh = state.load_catalog() => fresh,\n        };\n"
        "        " + FIRST_LOAD_LATE + delay(ms) + "\n        }\n",
    )] for how, delay in [("thread", THREAD), ("xthread", XTHREAD)] for ms in (700, 6000, 12000)
       if not (how == "thread" and ms == 700)},
    **{f"retry-wait-stop-late-thread-{ms}": [(
        W,
        "        () = turn.token.cancelled() => {}\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
        "        () = turn.token.cancelled() => std::thread::sleep(std::time::Duration::from_millis(" + str(ms) + ")),\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
    )] for ms in (500, 6000, 12000)},
    **{f"retry-wait-stop-late-xthread-{ms}": [(
        W,
        "        () = turn.token.cancelled() => {}\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
        "        () = turn.token.cancelled() => {\n" + XTHREAD(ms) + "\n        }\n"
        "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}\n",
    )] for ms in (500, 6000, 12000)},
    # Round 4: the watcher stopped only on the normal path (round 3's shape),
    # and the wall bound not asserted.
    "watcher-guard-removed": [(
        W,
        "            // Dropped as this closure ends or unwinds, before the scope\n"
        "            // joins the watcher.\n"
        "            let _stop = StopWatching(&watching);\n"
        "            run()\n",
        "            let out = run();\n"
        "            watching.store(false, Ordering::Release);\n"
        "            out\n",
    )],
    "bound-not-asserted": [(W, "                took < STOP_TO_END,\n", "                took < Duration::MAX,\n")],
    # QUESTIONS-20's M1, for test C's turn that asks twenty.
    "questions-cap-back-to-4": [(Q, "pub const MAX_QUESTIONS: usize = 20;", "pub const MAX_QUESTIONS: usize = 4;")],
}


def env():
    out = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        out.pop(name, None)
    return out


def git(clone, *args):
    return subprocess.run(["git", "-C", clone, *args], env=env(), capture_output=True, text=True)


def apply(clone, edits):
    for path, old, new in edits:
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as handle:
            text = handle.read()
        count = text.count(old)
        if count != 1:
            return f"{path}: the mutant's text matches {count} times"
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(text.replace(old, new))
    return None


def bounded(clone, args):
    proc = subprocess.Popen(args, cwd=clone, env=env(), stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=LIMIT)
        return proc.returncode, out, False
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, _ = proc.communicate()
        return None, out, True


def main():
    clone, label, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    only = sys.argv[4:] or list(MUTANTS)
    head = git(clone, "rev-parse", "--short=8", "HEAD").stdout.strip()
    lines = [f"# {label}: mutants.py on a git clone --no-local at {head}; the catalog-wait "
             f"tests (A to G) per mutant -- the whole lib suite for {sorted(FULL_LIB)} -- "
             f"bounded at {LIMIT} s"]
    for name in only:
        git(clone, "checkout", "--", ".")
        refused = apply(clone, MUTANTS[name])
        if refused:
            lines.append(f"{label} {name}: NOT APPLIED ({refused})")
            print(lines[-1], flush=True)
            continue
        rc, out, _ = bounded(clone, ["cargo", "test", "--locked", "--lib", "--no-run"])
        if rc != 0:
            lines.append(f"{label} {name}: BUILD FAILED")
            lines += ["    " + l for l in out.splitlines() if l.startswith("error")][:5]
            print(lines[-1], flush=True)
            git(clone, "checkout", "--", ".")
            continue
        args = ["cargo", "test", "--locked", "--lib"]
        if name not in FULL_LIB:
            args += ["--", "--exact", *TESTS]
        rc, out, hung = bounded(clone, args)
        failed = re.findall(r"^test (\S+) \.\.\. FAILED$", out, re.M)
        verdict = "timeout" if hung else ("SURVIVED" if rc == 0 else "killed")
        named = " ".join(SHORT.get(t, t) for t in failed)
        lines.append(f"{label} {name}: {verdict}" + (f" (failed: {named})" if failed else ""))
        lines += ["    " + l for l in out.splitlines()
                  if l.startswith(("  left:", " right:", "test result"))]
        print(lines[-1 - sum(1 for l in out.splitlines() if l.startswith(("  left:", " right:", "test result")))],
              flush=True)
        git(clone, "checkout", "--", ".")
    status = git(clone, "status", "--porcelain").stdout
    lines.append(f"clone after: {'clean' if not status.strip() else 'DIRTY: ' + status}")
    with open(out_path, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print(lines[-1])


main()
