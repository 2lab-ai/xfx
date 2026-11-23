"""The re-check's mutant, run as it reads: in the unwind test, an assertion
that fails before the disposition is put back. On a throwaway clone (never
the worktree), the line

    assert!(unwound.is_err(), "the waits did not unwind");

becomes one that fails, and a test added after it in name order,
`zz_sigurg_after_the_unwind_test`, reads SIGURG's disposition. The two run
alone, one thread, the unwind test first; the reader prints whether SIGURG
was left ignored. The clone is put back with `git checkout`.

    python3 -I literal.py <clone> <cargo-target-dir>
Prints the two tests' results and the reader's line; prints no path.
"""
import os
import re
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target
path = os.path.join(clone, "tests", "tui.rs")
OLD = '    assert!(unwound.is_err(), "the waits did not unwind");\n'
NEW = '    assert!(unwound.is_ok(), "an assertion that fails before the put-back");\n'
READER = '''
#[test]
fn zz_sigurg_after_the_unwind_test() {
    let _alone = SIGURG_IN_USE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (handler, _) = sigurg_disposition();
    let ignored = handler == libc::SIG_IGN;
    println!("SIGURG after the unwind test: {}", if ignored { "left ignored" } else { "not ignored" });
    assert!(!ignored, "the unwind test left SIGURG ignored");
}
'''
text = open(path).read()
assert text.count(OLD) == 1
open(path, "w").write(text.replace(OLD, NEW) + READER)
try:
    done = subprocess.run(["cargo", "test", "--locked", "--test", "tui", "--",
                           "--test-threads=1", "--nocapture",
                           "a_wait_that_unwinds_under_signals", "zz_sigurg_after_the_unwind_test"],
                          cwd=clone, env=env, capture_output=True, text=True, timeout=1200)
finally:
    subprocess.run(["git", "-C", clone, "checkout", "--", "tests/tui.rs"], env=env, check=True)
out = done.stdout + done.stderr
for line in out.splitlines():
    if re.search(r"zz_sigurg|SIGURG after|a_wait_that_unwinds|^test result|the unwind test left", line):
        print(line[:200])
