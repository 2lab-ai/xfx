"""The copy-errand x program-status join test, against mutants and against
each unit alone: each a change in a throwaway clone, then the test.

usage: python3 -I -B join_mutants.py <clone> <train-head> <l-u1u2> <flake-copyend> > join-mutants.txt

<clone> is a `git clone --no-local` of the repository, built in its own
target directory, with its own TMPDIR (<clone>/.t), so a fake tool a run
leaves behind is told from every other worker's by its path. Rows:

- the test at <train-head>, unchanged: it must pass;
- each mutant at <train-head>, one exact replacement: the test must fail
  (KILLED), and SURVIVED is a hole; NOT BUILT is no verdict (J4's frame is
  flushed: `std::io::stdout()` is line-buffered, and an unflushed frame
  would reach the terminal only at the process's exit, after the restore --
  a first run without the flush was killed by the after-the-restore check
  instead of the after-the-copy one);
- the test's text added, unchanged, to <l-u1u2> (L-U1+U2 alone: the old
  Errands drop, the program status clear in the restore) and to
  <flake-copyend> (FLAKE-COPYEND alone: its `Errands::end`, a restore with no
  clear): it must pass on each, so the order is each unit's and the train's.

Each run is `cargo test --locked --test tui <name> -- --exact`. The fake
`osascript` that hangs `exec`s `sleep 30`, which names no directory, so a
tool a run leaves behind cannot be told from another worker's: each run
counts the `sleep 30` processes whose parent is gone that were not there
before it, and kills none (each ends itself within 30 s).
"""

import os
import re
import subprocess
import sys
import time

TEST = "a_copy_running_at_the_end_is_given_up_after_the_last_frame_and_before_the_restore"
MOD, COPY = "src/tui/mod.rs", "src/tui/copy.rs"
GRACE_LINE = "        self.end(|began| began.elapsed() >= GRACE);\n"

MUTANTS = [
    ("J1 the restore written before the shell drops (before the copy's end)", MOD,
     "    Ok(Exit { code, handoff })\n",
     "    let _ = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"
     "    Ok(Exit { code, handoff })\n"),
    ("J2 a session's end leaves its copies running (the errands' drop detaches them)", COPY,
     GRACE_LINE, "        drop(std::mem::take(&mut *self.lock()));\n"),
    ("J3 a program status report after the restore", MOD,
     "    let restored = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n",
     "    let restored = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"
     "    let _ = io::stdout().write_all(b\"\\x1b]7501;state=idle:app=xfx\\x1b\\\\\");\n"),
    ("J4 a frame written half a second after the copy's end, before the restore", COPY,
     GRACE_LINE,
     GRACE_LINE + "        std::thread::sleep(Duration::from_millis(500));\n"
     "        let mut out = std::io::stdout();\n"
     "        let _ = std::io::Write::write_all(&mut out, b\"\\x1b[?2026h\\x1b[?2026l\");\n"
     "        let _ = std::io::Write::flush(&mut out);\n"),
]


def git(clone, *args):
    return subprocess.run(["git", "-C", clone, *args], check=True, capture_output=True, text=True).stdout


def orphans():
    out = subprocess.run(["ps", "-U", str(os.getuid()), "-o", "pid=,ppid=,command="],
                         capture_output=True, text=True).stdout
    found = set()
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[1] == "1" and parts[2].strip() == "sleep 30":
            found.add(int(parts[0]))
    return found


def run(clone, name, expect):
    tmp = os.path.join(clone, ".t")
    os.makedirs(tmp, exist_ok=True)
    env = dict(os.environ, TMPDIR=tmp)
    before = orphans()
    began = time.time()
    r = subprocess.run(["cargo", "test", "--locked", "--test", "tui", TEST, "--", "--exact"], cwd=clone,
                       capture_output=True, text=True, timeout=3600, env=env)
    took = time.time() - began
    text = r.stdout + r.stderr
    lines = [line for line in text.strip().split("\n") if line]
    result = [line for line in lines if line.startswith("test result:")]
    said = [lines[i + 1].strip() for i, line in enumerate(lines[:-1]) if "panicked at" in line]
    ran = re.search(r"running (\d+) test", text)
    left = orphans() - before
    built = "could not compile" not in text
    if not built:
        verdict = "NOT BUILT"
    elif ran is None or ran.group(1) != "1":
        verdict = "NOT RUN (%s)" % (ran.group(0) if ran else "no test ran")
    elif expect == "pass":
        verdict = "PASSED" if r.returncode == 0 else "FAILED"
    else:
        verdict = "KILLED" if r.returncode else "SURVIVED"
    print("%s: %s (exit %d, %.0f s: %s%s)%s" % (
        name, verdict, r.returncode, took, result[-1] if result else lines[-1],
        (" | said: " + said[-1][:220]) if said else "",
        (" [%d orphaned sleep 30 appeared during it]" % len(left)) if left else ""), flush=True)


def main(clone, head, lu, flake):
    git(clone, "checkout", "-q", "--detach", head)
    print("# the test at the train head %s" % head[:8], flush=True)
    run(clone, "unchanged", "pass")
    for name, path, old, new in MUTANTS:
        full = os.path.join(clone, path)
        text = open(full).read()
        if text.count(old) != 1:
            print("%s: NOT APPLIED (%d matches)" % (name, text.count(old)), flush=True)
            continue
        open(full, "w").write(text.replace(old, new))
        run(clone, name, "fail")
        git(clone, "checkout", "--", ".")
    tests = git(clone, "show", "%s:tests/tui.rs" % head)
    start = tests.index("#[cfg(target_os = \"macos\")]\n#[test]\nfn %s()" % TEST)
    end = tests.index("\n}\n", start) + 3
    body = tests[start:end]
    for label, commit in (("L-U1+U2 alone", lu), ("FLAKE-COPYEND alone", flake)):
        git(clone, "checkout", "-q", "--detach", commit)
        full = os.path.join(clone, "tests/tui.rs")
        text = open(full).read()
        anchor = "\n// ---------------------------------------------------------------------------\n// input typed before the session is up\n"
        assert text.count(anchor) == 1, label
        open(full, "w").write(text.replace(anchor, "\n" + body + anchor, 1))
        print("# the test's text added to %s %s" % (label, commit[:8]), flush=True)
        run(clone, label, "pass")
        git(clone, "checkout", "--", ".")


if __name__ == "__main__":
    main(*sys.argv[1:5])
