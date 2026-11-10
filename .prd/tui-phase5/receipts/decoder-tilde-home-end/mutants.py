"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-VERTICAL-SPELLINGS' mutants.py, with this unit's mutants: one per
spelling (M1-M6), then the `; 1` refusal and the Home/End sides of the
modified table."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
BARE_HOME = '            b"1" | b"7" => Input::Action(Action::Home),\n'
BARE_END = '            b"4" | b"8" => Input::Action(Action::End),\n'
MOD_HOME = '        b"1" | b"7" => cursor(b\'H\', mods),\n'
MOD_END = '        b"4" | b"8" => cursor(b\'F\', mods),\n'
GUARD = "    if mask == 0 {\n        return Input::Action(Action::Ignore);\n    }\n"
MUTANTS = [
    ("M1 CSI 7 ~ is not Home (the base's reading)", I, BARE_HOME, BARE_HOME.replace(' | b"7"', "")),
    ("M2 CSI 8 ~ is not End", I, BARE_END, BARE_END.replace(' | b"8"', "")),
    ("M3 CSI 1 ; <mod> ~ is not Home", I, MOD_HOME, MOD_HOME.replace('b"1" | ', "")),
    ("M4 CSI 4 ; <mod> ~ is not End", I, MOD_END, MOD_END.replace('b"4" | ', "")),
    ("M5 CSI 7 ; <mod> ~ is not Home", I, MOD_HOME, MOD_HOME.replace(' | b"7"', "")),
    ("M6 CSI 8 ; <mod> ~ is not End", I, MOD_END, MOD_END.replace(' | b"8"', "")),
    ("M7 a modifier field of 1 is the bare key (fx's reading)", I, GUARD, ""),
    ("M8 Home and End swapped under a modifier", I, MOD_HOME + "        b\"3\" => forward_delete(mods),\n" + MOD_END,
     MOD_HOME.replace("'H'", "'F'") + "        b\"3\" => forward_delete(mods),\n" + MOD_END.replace("'F'", "'H'")),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "tui::input::tests", "rxvts"],
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, path, old, new in MUTANTS:
    full = os.path.join(repo, path)
    src = open(full).read()
    if src.count(old) != 1:
        say(f"{name}: NOT APPLIED (pattern found {src.count(old)} times)"); continue
    open(full, "w").write(src.replace(old, new))
    verdict, why = "SURVIVED", ""
    t0 = time.time()
    try:
        for cmd in TESTS:
            try:
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=900)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", ", ".join(failed[:4]); break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
