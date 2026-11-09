#!/usr/bin/env python3
"""CATALOG-REFRESH mutants: each one edit to the head's source, run on a
throwaway `git clone --no-local` of the branch (never the worktree), the
unit's tests run under bounded.py (300 s, 3 GiB), then the edit undone with
`git checkout -- src`. A mutant is killed when a test fails or it does not
build.

usage: python3 -I mutants.py <clone> <cargo target dir>
The clone and the target dir are arguments, so no path of this machine is
written here; run it under the suite lock."""
import os, subprocess, sys

HERE = os.path.dirname(os.path.realpath(__file__))
TESTS = [
    "provider::catalog_clock::",
    "provider::model::tests::",
    "tui::shell::tests::a_resume_or_a_fork_is_activity_for_the_catalog",
    "tui::shell::tests::settings_and_the_speed_commands_wait_for_the_catalog",
    "tui::shell::tests::a_reading_earlier_than_the_last_reads_start",
    "tui::shell::tests::rows_and_a_failure_read_before_the_machine_slept",
    "tui::shell::tests::the_columns_frames_wait_out_a_failed_catalogs_backoff",
    "tui::shell::tests::rows_a_slow_read_brought_are_read_again",
    "tui::shell::tests::a_speed_command_before_the_catalog_waits",
    "tui::shell::tests::a_bare_settings_opens_upstreams_menu",
    "tui::worker::tests::the_catalog_is_read_by_a_first_load",
    "tui::worker::tests::an_ask_with_no_trigger_over_nothing_read",
    "tui::worker::tests::a_prompt_",
]
CLOCK, MODEL = "src/provider/catalog_clock.rs", "src/provider/model.rs"
SHELL, BRIDGE, WORKER = "src/tui/shell.rs", "src/tui/bridge.rs", "src/tui/worker.rs"
# (name, file, old, new, how many times `old` stands in the file)
MUTANTS = [
    ("Instant restored: the catalog reads the awake clock", CLOCK,
     "            Source::System => system::boot(),\n            #[cfg(test)]\n            Source::Fake(machine) => machine.boot(),",
     "            Source::System => system::awake(),\n            #[cfg(test)]\n            Source::Fake(machine) => machine.awake(),", 1),
    ("Apple's catalog clock is Instant's CLOCK_UPTIME_RAW", CLOCK,
     "pub(super) const BOOT: libc::clockid_t = libc::CLOCK_MONOTONIC_RAW;",
     "pub(super) const BOOT: libc::clockid_t = libc::CLOCK_UPTIME_RAW;", 1),
    ("a reading earlier than the last read's start is not due", CLOCK,
     "self.since.is_some_and(|since| now < since) || now >= self.at",
     "now >= self.at", 1),
    ("the runtime does not tell when the last read began", MODEL,
     "            since: self.began,", "            since: None,", 1),
    ("the selector's reads are timed on the system clock, not its own", MODEL,
     "self.pending = Some(PendingLoad::start(fetcher, self.time.clone(), told));",
     "self.pending = Some(PendingLoad::start(fetcher, CatalogClock::system(), told));", 2),
    ("a prompt reads the system clock, not the band's", SHELL,
     "        // `super::worker`'s `await_failed_catalog_retry`).\n        let _ = self.note_catalog_activity(self.catalog_clock.now());",
     "        // `super::worker`'s `await_failed_catalog_retry`).\n        let _ = self.note_catalog_activity(CatalogClock::system().now());", 1),
    ("a resume or fork asks nothing", SHELL,
     "        // way ([`Self::note_catalog_activity`]).\n        let _ = self.note_catalog_activity(self.catalog_clock.now());\n",
     "        // way ([`Self::note_catalog_activity`]).\n", 1),
    ("/settings asks with activity's trigger", SHELL,
     "        // the band).\n        self.want_catalog(CatalogAsk::TOLD);",
     "        // the band).\n        self.want_catalog(CatalogAsk::AWAITED);", 1),
    ("the speed commands ask with activity's trigger", SHELL,
     "            self.held_speed.push(command);\n            self.want_catalog(CatalogAsk::TOLD);",
     "            self.held_speed.push(command);\n            self.want_catalog(CatalogAsk::AWAITED);", 1),
    ("TOLD carries activity's trigger", BRIDGE,
     "    pub(crate) const TOLD: Self = Self {\n        trigger: None,",
     "    pub(crate) const TOLD: Self = Self {\n        trigger: Some(crate::provider::model::LoadTrigger::Activity),", 1),
    ("an ask with no trigger reads as activity", WORKER,
     "    ask.trigger\n        .or_else(|| (held == Some(Held::Nothing)).then_some(LoadTrigger::Activity))",
     "    let _ = held;\n    ask.trigger.or(Some(LoadTrigger::Activity))", 1),
    ("an ask with no trigger starts no first load either", WORKER,
     "    ask.trigger\n        .or_else(|| (held == Some(Held::Nothing)).then_some(LoadTrigger::Activity))",
     "    let _ = held;\n    ask.trigger", 1),
    ("between work, an ask with no trigger starts no first load", WORKER,
     "        trigger_over(ask, held).filter(|trigger| state.selector.would_load(*trigger, now))",
     "        ask.trigger.filter(|trigger| state.selector.would_load(*trigger, now))", 1),
    ("mid-work, an ask with no trigger starts no first load", WORKER,
     "        let due = trigger_over(ask, held)",
     "        let due = ask.trigger", 1),
    ("mid-work, an ask with no trigger reads as activity", WORKER,
     "        let due = trigger_over(ask, held)",
     "        let due = ask.trigger.or(Some(LoadTrigger::Activity))", 1),
    ("the column's frame asks nothing (fx's modelCompletions does)", SHELL,
     "        // `picker_presentation.zig:914-921`).\n        let _ = self.note_catalog_activity(now);\n",
     "        // `picker_presentation.zig:914-921`).\n        let _ = now;\n", 1),
]

def run(clone, target, cmd, **kw):
    env = dict(os.environ, CARGO_TARGET_DIR=target)
    return subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True, **kw)

def main():
    clone, target = sys.argv[1], sys.argv[2]
    head = run(clone, target, ["git", "rev-parse", "--short", "HEAD"]).stdout.strip()
    print(f"# clone at {head}", flush=True)
    killed = 0
    for name, path, old, new, times in MUTANTS:
        file = os.path.join(clone, path)
        text = open(file).read()
        assert text.count(old) == times, f"{name}: the edit's text is not in {path} {times} time(s)"
        open(file, "w").write(text.replace(old, new))
        try:
            build = run(clone, target, ["cargo", "test", "--locked", "--lib", "--no-run"])
            if build.returncode != 0:
                verdict = "killed (does not build)"
                killed += 1
            else:
                test = run(clone, target, ["python3", "-I", os.path.join(HERE, "bounded.py"), "300", "3072", "--",
                                           "cargo", "test", "--locked", "--lib", "--", "--test-threads=2", *TESTS])
                failed = [l.split()[1] for l in test.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if test.returncode != 0:
                    killed += 1
                    verdict = "killed by " + (", ".join(t.split("::")[-1] for t in failed) or test.stderr.strip().splitlines()[-1])
                else:
                    verdict = "SURVIVED"
        finally:
            run(clone, target, ["git", "checkout", "--", "src"])
        print(f"- {name} ({path}): {verdict}", flush=True)
    print(f"{killed} of {len(MUTANTS)} killed")
    return 0 if killed == len(MUTANTS) else 1

sys.exit(main())
