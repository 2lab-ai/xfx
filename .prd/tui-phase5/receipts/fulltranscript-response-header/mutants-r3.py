"""usage: TMPDIR=<short dir> python3 -I mutants-r3.py "$TMPDIR/mut/repo" <out.txt>
Round 3's mutants, for its new assertions only (this folder's mutants.py,
round 1's and round 2's, is unchanged): the runner is mutants.py's. Never run
against the worktree: the clone must sit under `$TMPDIR/mut/`.
M1 `resolve_bookmark_row` drops both fallbacks (a bookmark with no mark keeps
the raw offset), M2 it drops the one after (only the nearest before), M3
every entry that lays out as nothing is marked again (round 1's `measure`),
M4 only a call that lays out as nothing is marked. Each runs round 3's two
new tests alone, so a kill is theirs (position.rs's own bookmark test, which
also pins the fallbacks, is left out)."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
F = "src/tui/full_transcript.rs"
P = "src/tui/full_transcript/position.rs"
S = "src/tui/shell.rs"
FALLBACK = "    preceding.or(following).map(|item| item.row)\n"
EMPTY = "            if rows.is_empty() {\n                continue;\n            }\n            if total > 0 {\n"
SHELL_END = "        self.end_transcript_line();\n    }\n\n    /// Ends an open prose block on one blank row.\n"
MUTANTS = [
    ("M1 resolve_bookmark_row drops both fallbacks", P, FALLBACK,
     "    let _ = (preceding, following);\n    None\n"),
    ("M2 resolve_bookmark_row drops the entry after", P, FALLBACK,
     "    let _ = following;\n    preceding.map(|item| item.row)\n"),
    ("M3 every entry that lays out as nothing is marked (round 1's measure)", F, EMPTY,
     "            if rows.is_empty() {\n                items.push(ItemRow { id, row: total });\n                continue;\n            }\n            if total > 0 {\n"),
    ("M4 a call that lays out as nothing is marked", F, EMPTY,
     "            if rows.is_empty() {\n                if matches!(entry.body, Body::Tool(_)) {\n"
     "                    items.push(ItemRow { id, row: total });\n                }\n                continue;\n            }\n            if total > 0 {\n"),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--",
     "a_reader_near_an_entry_that_lays_out_as_nothing_holds_when_it_becomes_visible",
     "a_reader_at_the_tail_follows_an_entry_that_lays_out_as_nothing_when_it_becomes_visible",
     ],
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
                    f.removeprefix("tui::full_transcript::tests::").removeprefix("tui::shell::tests::").removeprefix("tui::full_transcript::position::tests::")
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
