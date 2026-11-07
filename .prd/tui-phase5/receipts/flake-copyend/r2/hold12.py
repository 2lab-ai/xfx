"""FLAKE-COPYEND r2's start-lock harness for the two cases the round-1
sweep got wrong: in a throwaway clone, each named case is given a first
statement that has another thread take the process's start lock
(`crate::process::hold_every_start`, process.rs SPAWNING) and hold it for
12 s -- what another test's slow start does in a loaded suite (a child
start of about 17 s on CI: repeated-failures row 19). Nothing else in the
case changes.

usage: python3 -I hold12.py <clone>
Then: cargo test --locked --lib <case> -- --exact, for each case below.

The cases (the body each has at the clone's commit is the one run):
  clipboard::tests::tool::a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure
  tui::trace::tests::an_ending_session_joins_a_report_whole_and_gives_its_copy_up
"""

import sys

HOLD = """
        // hold12.py: another start holds the process's start lock for 12 s.
        {
            let (taken, waiting) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let lock = crate::process::hold_every_start();
                taken.send(()).expect("the case is waiting");
                std::thread::sleep(std::time::Duration::from_secs(12));
                drop(lock);
            });
            waiting.recv().expect("the lock was taken");
        }
"""

CASES = [
    ("src/clipboard.rs", "fn a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure() {\n"),
    ("src/tui/trace.rs", "fn an_ending_session_joins_a_report_whole_and_gives_its_copy_up() {\n"),
]

clone = sys.argv[1]
for path, line in CASES:
    full = clone + "/" + path
    text = open(full).read()
    assert text.count(line) == 1, (path, text.count(line))
    open(full, "w").write(text.replace(line, line + HOLD))
    print("hold added: %s" % line.strip())
