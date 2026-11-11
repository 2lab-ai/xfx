"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
MODIFIED-PAGE-KEYS' runner, with this unit's mutants, each a way the record
could split an answer otherwise than fx's store does: M1 the base's reading
(a block joins the open run, and the text after it with it), M2 a block's
entry headed `Response`, M3 a blank line after a block opening nothing (the
run then stamped at its first prose), M4 a block opening a `Response` entry
of its own, M5 a block's entry laid out as nothing, M6 `/trace` naming every
block `assistant`.
Round 2, at its code head: the six again (M6 on the label match that now
names every variant) and three for the round's lines -- M7 every entry that
lays out as nothing is a reading place again (round 1's code), M8 a call
that lays out as nothing is one, M9 `/trace` names a rule `assistant`."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
F = "src/tui/full_transcript.rs"
JOIN = "            }) if text => {\n                events.push(event.clone());\n"
STRUCTURE_ARM = ("            Some(Entry {\n                body: Body::Structure(_),\n"
                 "                ..\n            }) if text => {}\n")
HEADLESS = "            Body::Structure(event) => (\n                None,\n"
PUSHED = "            Body::Structure(event.clone())\n"
LAID = "                written(answer(std::slice::from_ref(event), cols, palette)),\n"
NAMES = ('                        Body::Structure(markdown::Event::Table(_)) => "assistant table",\n'
         '                        Body::Structure(markdown::Event::CodeBlock(_)) => "assistant code block",\n'
         '                        Body::Structure(markdown::Event::ThematicRule) => "assistant thematic rule",\n')
RULE = '                        Body::Structure(markdown::Event::ThematicRule) => "assistant thematic rule",\n'
EMPTY = "            if rows.is_empty() {\n                continue;\n            }\n            if total > 0 {\n"
MUTANTS = [
    ("M1 a block joins the open run, and the text after it with it (the base's reading)", F, JOIN,
     "            }) => {\n                events.push(event.clone());\n"),
    ("M2 a block's entry is headed Response", F, HEADLESS,
     "            Body::Structure(event) => (\n                Some(Kind::Response),\n"),
    ("M3 a blank line after a block opens nothing (the run stamped at its first prose)", F, STRUCTURE_ARM, ""),
    ("M4 a block opens a Response entry of its own", F, PUSHED,
     "            Body::Response(vec![event.clone()])\n"),
    ("M5 a block's entry lays out as nothing", F, LAID, "                written(Vec::new()),\n"),
    ("M6 /trace names every block assistant", F, NAMES,
     '                        Body::Structure(markdown::Event::Table(_) | markdown::Event::CodeBlock(_) | markdown::Event::ThematicRule) => "assistant",\n'),
    # Round 2.
    ("M7 every entry that lays out as nothing is a reading place (round 1's code)", F, EMPTY,
     "            if rows.is_empty() {\n                items.push(ItemRow { id, row: total });\n                continue;\n            }\n            if total > 0 {\n"),
    ("M8 a call that lays out as nothing is a reading place", F, EMPTY,
     "            if rows.is_empty() {\n                if matches!(entry.body, Body::Tool(_)) {\n"
     "                    items.push(ItemRow { id, row: total });\n                }\n                continue;\n            }\n            if total > 0 {\n"),
    ("M9 /trace names a rule assistant", F, RULE,
     '                        Body::Structure(markdown::Event::ThematicRule) => "assistant",\n'),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "tui::full_transcript::tests",
     "an_answers_blocks_split_its_ctrl_o_record", "a_resumed_answers_runs_carry",
     "trace::"],
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
                verdict, why = "KILLED", ", ".join(
                    f.removeprefix("tui::full_transcript::tests::").removeprefix("tui::shell::tests::")
                    for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
