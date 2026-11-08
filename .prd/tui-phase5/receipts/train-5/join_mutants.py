"""The copy-errand x program-status join test (train 5 r2: `/copy`, judged on
the whole terminal stream after the session's exit), against mutants, with
its observer delayed across the shutdown, and on each unit alone; and, for
contrast, round 1's sampled test with its observer delayed the same way.

usage: python3 -I -B join_mutants.py <clone> <train-head> <r1-head> <l-u1u2> <flake-copyend> > join-mutants.txt

<clone> is a `git clone --no-local` of the repository, built in its own
target directory, each run with its own TMPDIR (<clone>/.t). Rows:

- the test at <train-head>, unchanged: it must pass;
- each mutant at <train-head>, exact replacements in the product (one, or
  for J6 two in one file): the test must fail (KILLED); SURVIVED is a hole;
  NOT BUILT is no verdict. The line the test said when it failed names the
  check that caught it;
- the control (gpt6): the same test with its observer delayed across the
  shutdown -- `std::thread::sleep(3 s)` right after its Ctrl-D, so the
  session has exited before the test reads anything again -- unchanged (it
  must pass) and against every mutant (each must still be KILLED);
- the test's text and the fake clipboard (`tests/support/fake_clipboard.rs`)
  as the train has them, added unchanged to <l-u1u2> (the old `Errands`
  drop, the clear in the restore) and to <flake-copyend> (its `end`, a
  restore with no clear): it must pass on each;
- for contrast, round 1's test at <r1-head> (`/trace`, looks taken while the
  session runs), with the same 3 s delay after its Ctrl-D, against round 1's
  J1 and J4: a sampled test whose observer is delayed has nothing to sample,
  and SURVIVED there is the blind spot the reviews named.

Each run is `cargo test --locked --test tui <name> -- --exact`. A copy tool
a mutant leaves behind (J2) ends itself when the case's directory goes; the
driver counts any `sleep 30` (round 1's tool) whose parent is gone that
appears during a run, and kills none.
"""

import os
import re
import subprocess
import sys
import time

TEST = "a_copy_running_at_the_end_is_given_up_after_the_last_frame_and_before_the_restore"
MOD, COPY = "src/tui/mod.rs", "src/tui/copy.rs"
GRACE_LINE = "        self.end(|began| began.elapsed() >= GRACE);\n"


def after_end(payload):
    return (GRACE_LINE + "        let mut out = std::io::stdout();\n"
            "        let _ = std::io::Write::write_all(&mut out, b\"%s\");\n"
            "        let _ = std::io::Write::flush(&mut out);\n" % payload)


J1 = ("J1 the restore written before the shell drops (before the copy's end)", MOD,
      "    Ok(Exit { code, handoff })\n",
      "    let _ = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"
      "    Ok(Exit { code, handoff })\n")
MUTANTS = [
    J1,
    ("J2 a session's end leaves its copies running (the errands' drop detaches them)", COPY,
     GRACE_LINE, "        drop(std::mem::take(&mut *self.lock()));\n"),
    ("J3 a program status report after the restore", MOD,
     "    let restored = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n",
     "    let restored = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"
     "    let _ = io::stdout().write_all(b\"\\x1b]7501;state=idle:app=xfx\\x1b\\\\\");\n"),
    ("J4 a frame written right after the copy's end, before the restore (no delay)", COPY,
     GRACE_LINE, after_end("\\x1b[?2026h\\x1b[?2026l")),
    ("J5 a program status report written right after the copy's end, before the restore", COPY,
     GRACE_LINE, after_end("\\x1b]7501;state=idle:app=xfx\\x1b\\\\")),
    # J1 writes a second restore, which the restore count catches first; J6
    # moves the one restore ahead of the copy's end instead, so only the
    # no-heartbeat-after-the-restore check stands between it and a pass.
    ("J6 the one restore moved ahead of the copy's end (written in hold, none after it)", MOD,
     ("    Ok(Exit { code, handoff })\n", "    let restored = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"),
     ("    let _ = term::shutdown(band.painted_top(), band.on_alternate(), band.screen_size());\n"
      "    Ok(Exit { code, handoff })\n", "    let restored: io::Result<()> = Ok(());\n")),
]
# Round 1's J4, as round 1 ran it (a frame half a second after the copy's end).
R1_J4 = ("round 1's J4 a frame half a second after the copy's end", COPY, GRACE_LINE,
         GRACE_LINE + "        std::thread::sleep(Duration::from_millis(500));\n"
         "        let mut out = std::io::stdout();\n"
         "        let _ = std::io::Write::write_all(&mut out, b\"\\x1b[?2026h\\x1b[?2026l\");\n"
         "        let _ = std::io::Write::flush(&mut out);\n")
QUIT = "    session.type_bytes(&[0x04]);\n"
DELAY = QUIT + "    std::thread::sleep(std::time::Duration::from_secs(3));\n"
ANCHOR = ("\n// ---------------------------------------------------------------------------\n"
          "// input typed before the session is up\n")


def git(clone, *args):
    return subprocess.run(["git", "-C", clone, *args], check=True, capture_output=True, text=True).stdout


def orphans():
    out = subprocess.run(["ps", "-U", str(os.getuid()), "-o", "pid=,ppid=,command="],
                         capture_output=True, text=True).stdout
    return {int(p[0]) for p in (l.split(None, 2) for l in out.splitlines())
            if len(p) == 3 and p[1] == "1" and p[2].strip() == "sleep 30"}


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
    if "could not compile" in text:
        verdict = "NOT BUILT"
    elif ran is None or ran.group(1) != "1":
        verdict = "NOT RUN (%s)" % (ran.group(0) if ran else "no test ran")
    elif expect == "pass":
        verdict = "PASSED" if r.returncode == 0 else "FAILED"
    else:
        verdict = "KILLED" if r.returncode else "SURVIVED"
    print("%s: %s (exit %d, %.0f s: %s%s)%s" % (
        name, verdict, r.returncode, took, result[-1] if result else lines[-1],
        (" | said: " + said[-1][:200]) if said else "",
        (" [%d orphaned sleep 30 appeared during it]" % len(left)) if left else ""), flush=True)


def edit(clone, path, old, new, label):
    """One exact replacement, or several: `old` and `new` may be tuples of
    the same length, each pair applied in turn, each `old` found once."""
    full = os.path.join(clone, path)
    text = open(full).read()
    olds, news = (old, new) if isinstance(old, tuple) else ((old,), (new,))
    for o, n in zip(olds, news):
        if text.count(o) != 1:
            print("%s: NOT APPLIED (%d matches in %s)" % (label, text.count(o), path), flush=True)
            return False
        text = text.replace(o, n, 1)
    open(full, "w").write(text)
    return True


def mutants_at(clone, head, delayed):
    tag = " [observer delayed 3 s after Ctrl-D]" if delayed else ""
    git(clone, "checkout", "-q", "--detach", head)
    git(clone, "checkout", "--", ".")
    if delayed and not edit(clone, "tests/tui.rs", QUIT + "    assert_eq!(session.wait_exit().code(), Some(0));\n    assert!(!alive(tool), \"the copy's tool outlived the session\");",
                            DELAY + "    assert_eq!(session.wait_exit().code(), Some(0));\n    assert!(!alive(tool), \"the copy's tool outlived the session\");", "the delay"):
        return
    run(clone, "unchanged" + tag, "pass")
    for name, path, old, new in MUTANTS:
        if edit(clone, path, old, new, name):
            run(clone, name + tag, "fail")
            git(clone, "checkout", "--", path)
    git(clone, "checkout", "--", ".")


def main(clone, head, r1, lu, flake):
    print("# the test at the train head %s" % head[:8], flush=True)
    mutants_at(clone, head, False)
    print("# the control: the same, the observer delayed across the shutdown", flush=True)
    mutants_at(clone, head, True)
    tests = git(clone, "show", "%s:tests/tui.rs" % head)
    start = tests.index("#[test]\nfn %s()" % TEST)
    body = tests[start:tests.index("\n}\n", start) + 3]
    fake = git(clone, "show", "%s:tests/support/fake_clipboard.rs" % head)
    for label, commit in (("L-U1+U2 alone", lu), ("FLAKE-COPYEND alone", flake)):
        git(clone, "checkout", "-q", "--detach", commit)
        if edit(clone, "tests/tui.rs", ANCHOR, "\n" + body + ANCHOR, label):
            open(os.path.join(clone, "tests/support/fake_clipboard.rs"), "w").write(fake)
            print("# the test's text and the train's fake clipboard added to %s %s" % (label, commit[:8]), flush=True)
            run(clone, label, "pass")
        git(clone, "checkout", "--", ".")
    print("# contrast: round 1's sampled test at %s, its observer delayed 3 s after Ctrl-D" % r1[:8], flush=True)
    git(clone, "checkout", "-q", "--detach", r1)
    if edit(clone, "tests/tui.rs", "    session.type_bytes(&[0x04]);\n    let mut gone_with",
            "    session.type_bytes(&[0x04]);\n    std::thread::sleep(std::time::Duration::from_secs(3));\n    let mut gone_with",
            "round 1's delay"):
        run(clone, "round 1's test, unchanged, delayed", "pass")
        for name, path, old, new in (J1, R1_J4):
            if edit(clone, path, old, new, name):
                run(clone, "round 1's test, delayed, against " + name, "fail")
                git(clone, "checkout", "--", path)
    git(clone, "checkout", "--", ".")


if __name__ == "__main__":
    main(*sys.argv[1:6])
