#!/usr/bin/env python3
"""Mutants of the COMPACT-NOTICE-LATE fix, each on a throwaway clone.

usage: python3 -I mutants.py <clone-dir> <out-file>
The clone is a `git clone --no-local` of the unit's head. Each mutant is a
string replacement in one file; the clone is reset (`git checkout -- .`)
before each. Every mutant runs the lib tests matching compact/stop/cancel
and the pty tests matching compact, each under a time limit (round 2 and 3: lib filters compact, stop, cancel, drop, reset; a mutant of several places lists them with XXSPLITXX); a mutant is
killed when any test fails (or the build fails), and survives otherwise.
"""
import subprocess, sys, os

clone, out = sys.argv[1], sys.argv[2]
SHELL = "src/tui/shell.rs"
WORKER = "src/tui/worker.rs"
COMPACTION = "src/tui/compaction.rs"

CANCEL_BLOCK = """        if self.cancelling && self.work.outstanding() == 0 && self.tally.started.is_none() {
            self.release_late(false);
            self.cancelling = false;
            self.stop_reads_end = false;
            self.compaction_ended_turn = false;
            self.cancel_owed = None;
        }
"""
ABANDON = """            let told = !matches!(item, TurnWork::Compact)
                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));
            if told {
                self.outstanding.fetch_sub(1, Ordering::Release);
            } else {
                self.owed += 1;
            }"""
TELL = """            bridge::send_terminal(events, UiEvent::Compaction(Update::Dropped)).await;
            self.owed -= 1;
            self.outstanding.fetch_sub(1, Ordering::Release);"""
LOOP_TOP = """        // taken.
        queue.tell_dropped(events).await;
"""
ABANDON_ARM = """            Taken::Abandon(through) => {
                queue.abandon(through, events);
                continue;
            }"""
MUTANTS = [
    ("N1 the runtime never tells a drop (the base's abandon)", WORKER, ABANDON,
     """            let _ = (&item, events);
            self.outstanding.fetch_sub(1, Ordering::Release);"""),
    ("N2 told after the place goes back (no room)", WORKER, TELL,
     """            self.owed -= 1;
            self.outstanding.fetch_sub(1, Ordering::Release);
            bridge::send_terminal(events, UiEvent::Compaction(Update::Dropped)).await;"""),
    ("N3 no telling at the loop's top", WORKER, LOOP_TOP, """        // taken.
"""),
    ("N4 no telling before a reset's fence", WORKER, """    queue.tell_dropped(events).await;
    drop_conversation(state, events, through).await;
}""", """    let _ = &queue;
    drop_conversation(state, events, through).await;
}"""),
    ("N5 the word at the stop is Settled(Cancelled)", WORKER,
     """                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));""",
     """                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Settled(Feedback::Cancelled)));"""),
    ("N5b the word with no room is Settled(Cancelled)", WORKER,
     """            bridge::send_terminal(events, UiEvent::Compaction(Update::Dropped)).await;""",
     """            bridge::send_terminal(events, UiEvent::Compaction(Update::Settled(Feedback::Cancelled))).await;"""),
    ("N6 the band's settle-time inference back", SHELL, CANCEL_BLOCK, CANCEL_BLOCK + """        if self.work.outstanding() == 0 {
            if let Some(compaction) = self.compaction.as_mut() {
                if compaction.stopped() {
                    compaction.settle(compaction::Feedback::Cancelled, now);
                }
            }
        }
"""),
    ("N7 a line after the stop is not noted", COMPACTION, """        if self.stopped() {
            self.answered = true;
        }""", """        let _ = self.stopped();"""),
    ("N8 any line is noted, not only one after the stop", COMPACTION, """        if self.stopped() {
            self.answered = true;
        }""", """        self.answered = true;"""),
    ("N9 a drop settles any compaction", SHELL, """                    .filter(|compaction| compaction.stopped())
                else {
                    return;
                };
                compaction.settle(compaction::Feedback::Cancelled, Instant::now());
                let answered""", """                    .filter(|_| true)
                else {
                    return;
                };
                compaction.settle(compaction::Feedback::Cancelled, Instant::now());
                let answered"""),
    ("N10 a drop notes no program status", SHELL, """                let answered = compaction.answered();
                self.program_status
                    .compaction(program_status::of_compaction(
                        compaction::Feedback::Cancelled,
                    ));""", """                let answered = compaction.answered();"""),
    ("N11 a drop always dismisses", SHELL, """                if answered {
                    self.compaction = None;
                }""", """                let _ = answered;
                self.compaction = None;"""),
    ("N12 a drop never dismisses", SHELL, """                if answered {
                    self.compaction = None;
                }""", """                let _ = answered;"""),
    ("N13 half-revert: told only in the loop's Abandon and Reset arms", WORKER, LOOP_TOP + "XXSPLITXX" + ABANDON_ARM,
     """        // taken.
""" + "XXSPLITXX" + """            Taken::Abandon(through) => {
                queue.abandon(through, events);
                queue.tell_dropped(events).await;
                continue;
            }"""),
    ("N14 never told at the stop (always waits, round 2)", WORKER,
     """                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));""",
     """                || { let _ = events; false };"""),
    ("N15 no room: the place goes back at the drop anyway", WORKER, """            } else {
                self.owed += 1;
            }""", """            } else {
                self.owed += 1;
                self.outstanding.fetch_sub(1, Ordering::Release);
            }"""),
    ("N15b no room: tell_dropped gives back no place", WORKER, TELL,
     """            bridge::send_terminal(events, UiEvent::Compaction(Update::Dropped)).await;
            self.owed -= 1;"""),
    ("N16 every dropped item is told a Dropped", WORKER,
     """            let told = !matches!(item, TurnWork::Compact)
                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));""",
     """            let _ = &item;
            let told = bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));"""),
    ("N17 told at the stop, the place kept", WORKER, """            if told {
                self.outstanding.fetch_sub(1, Ordering::Release);
            } else {
                self.owed += 1;
            }""", """            if told && !matches!(item, TurnWork::Compact) {
                self.outstanding.fetch_sub(1, Ordering::Release);
            } else if !told {
                self.owed += 1;
            }"""),
    ("N18 /compact during a reset: no NoOp from the reset", SHELL,
     """        if self.work.status().history_turns == 0 || (resetting && self.work.outstanding() == 0) {""",
     """        if self.work.status().history_turns == 0 {"""),
    ("N19 /compact during a reset: not busy while work runs", SHELL,
     """        if self.activity.working() || self.waiting_prompts().next().is_some() || resetting {""",
     """        if self.activity.working() || self.waiting_prompts().next().is_some() {"""),
]

env = dict(os.environ, TMPDIR="<tmp>", CARGO_TARGET_DIR=os.path.join(clone, "target"))

def run(cmd, limit):
    try:
        p = subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True, timeout=limit)
        return p.returncode, p.stdout + p.stderr
    except subprocess.TimeoutExpired as e:
        return "timeout", ""

def failing(text):
    return [l for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")][:6]

with open(out, "w") as f:
    head = subprocess.run(["git", "rev-parse", "--short=8", "HEAD"], cwd=clone, capture_output=True, text=True).stdout.strip()
    f.write(f"# mutants of the fix, on a clone at {head}\n")
    for name, path, old, new in MUTANTS:
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
        src = open(os.path.join(clone, path)).read()
        # A mutant of more than one place: its parts, XXSPLITXX between them.
        pairs = list(zip(old.split("XXSPLITXX"), new.split("XXSPLITXX")))
        counts = [src.count(part) for part, _ in pairs]
        if counts != [1] * len(pairs):
            f.write(f"{name}: NOT APPLIED (anchor counts {counts})\n"); f.flush(); continue
        for part, replacement in pairs:
            src = src.replace(part, replacement)
        open(os.path.join(clone, path), "w").write(src)
        verdict, why = "survived", []
        for cmd, limit in (
            (["cargo", "test", "--locked", "--lib", "--", "compact", "stop", "cancel", "drop", "reset"], 1200),
            (["cargo", "test", "--locked", "--test", "tui", "--", "compact"], 1200),
        ):
            code, text = run(cmd, limit)
            if code == "timeout":
                verdict, why = "killed", [f"timeout in {' '.join(cmd[2:5])}"]; break
            if code != 0:
                verdict = "killed"
                why = failing(text) or [l for l in text.splitlines() if l.startswith("error")][:3]
                break
        f.write(f"{name}: {verdict}" + (": " + " | ".join(why) if why else "") + "\n"); f.flush()
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
    f.write("# done\n")
