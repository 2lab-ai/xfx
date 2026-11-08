#!/usr/bin/env python3
"""Train 7's join mutants, each on a throwaway `git clone --no-local` of the
head, never on the worktree: one textual change (exactly one occurrence),
the lib tests built, then the group's tests run with a timeout -- the
process group killed past it and the mutant recorded KILLED (hang) -- the
failing tests named, the file restored with `git checkout`, the clone clean
at the end.

    python3 -I -B join_mutants.py <clone> <out.txt> [<id>...]

Group T (join check 2, T-U4-F x ARGROW-CACHE): the merged `unit_at` and
`inline` (src/tui/toolgroup.rs) and the fast paths of src/tui/wrap.rs that
`inline` measures each unit through (`wrap::width` -> `painting` ->
`first_cluster`, `cluster_cells`); the tests: `cargo test --locked --lib
tui::` (T-U4's runner's filter: the row's tests, T-U4-F's Prepend tests,
train 6's row-cut join test, ARGROW-CACHE's wrap property tests).
Group J (join check 1, MODELS-REASK x STOP-NEXT-2): the band drops a
catalog answer read in a stop's window, and a turn's catalog wait not raced
against the turn's own stop; the tests: train 7's three join tests by name.
"""
import os
import re
import signal
import subprocess
import sys

TIMEOUT_S = 900
JOINS = ["a_catalog_answer_read_in_a_stops_window_reaches_the_band_and_leaves_the_stop_as_it_was",
         "a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number",
         "catalog_asks_around_a_stop_leave_its_count_and_the_next_start_its_own"]
FIND = ("                .find(|&(at, character)| character == '\\\\' && spelling_at(&rest[at..]).is_some())\n"
        "                .map_or(whole, |(at, _)| at)")
MUTANTS = [
    # Group T: T-U4-F's three (its t-u4-f/mutants.py), on the merged function.
    ("T1", "unit_at: a cluster keeps the spelling it took in (T-U4's rule)", "src/tui/toolgroup.rs",
     FIND, "                .find(|_| false)\n                .map_or(whole, |(at, _): (usize, char)| at)"),
    ("T2", "unit_at: a cluster ends before any `\\` it took in, spelling or not", "src/tui/toolgroup.rs",
     FIND, "                .find(|&(_, character)| character == '\\\\')\n                .map_or(whole, |(at, _)| at)"),
    ("T3", "unit_at: a cluster ends before its second character, whatever it is (T-U4-F's hang)", "src/tui/toolgroup.rs",
     FIND, "                .find(|_| true)\n                .map_or(whole, |(at, _)| at)"),
    # T-U4's own: a spelling's unit without what joins its last character;
    # the row measured raw, not spelled; the bound one cell short.
    ("T4", "unit_at: a spelling's unit ends at its last character, a mark after it left out", "src/tui/toolgroup.rs",
     "        Some(len) => len - 1 + cluster(&rest[len - 1..]),", "        Some(len) => len,"),
    ("T5", "inline: each unit measured raw, not as spelled", "src/tui/toolgroup.rs",
     "        cells += usize::from(super::wrap::width(&unit_spelled));",
     "        cells += usize::from(super::wrap::width(unit));"),
    ("T6", "inline: the bound one cell short (`>=`)", "src/tui/toolgroup.rs",
     "        if cells > INLINE_CELLS {", "        if cells >= INLINE_CELLS {"),
    # ARGROW-CACHE's fast paths, as `inline` reaches them through wrap::width.
    ("T7", "first_cluster: a printable ASCII byte is its own cluster even before a mark", "src/tui/wrap.rs",
     "            if (0x20..=0x7e).contains(first) && after.first().is_none_or(u8::is_ascii) =>",
     "            if (0x20..=0x7e).contains(first) =>"),
    ("T8", "cluster_cells: any one-byte cluster below DEL, a control too, one cell", "src/tui/wrap.rs",
     "        if (0x20..=0x7e).contains(byte) {\n            return 1;",
     "        if (0x00..=0x7e).contains(byte) {\n            return 1;"),
    # Group J.
    ("J1", "the band drops a catalog answer read under a stop (held like late text, never presented)", "src/tui/shell.rs",
     "            UiEvent::CatalogLoaded {\n                provider,\n                entries,\n                due,\n            } => {\n",
     "            UiEvent::CatalogLoaded {\n                provider,\n                entries,\n                due,\n            } => {\n"
     "                if self.cancelling {\n                    return;\n                }\n"),
    ("J2", "a turn's wait for a failed catalog's retry not raced against its stop", "src/tui/worker.rs",
     "        () = turn.token.cancelled() => {}\n        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}",
     "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}"),
]


def run(clone, args, timeout=None):
    proc = subprocess.Popen(args, cwd=clone, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=timeout)
        return proc.returncode, out, False
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, _ = proc.communicate()
        return None, out, True


def tests_of(ident):
    if ident.startswith("J"):
        return ["cargo", "test", "--locked", "--lib", "--", *JOINS]
    return ["cargo", "test", "--locked", "--lib", "tui::"]


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    only = set(sys.argv[3:])
    lines = [f"# join_mutants.py on a git clone --no-local at "
             f"{subprocess.run(['git', '-C', clone, 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True).stdout.strip()}"
             f"; each run's timeout {TIMEOUT_S} s (build first, outside it)"]
    for group in ("T", "J"):
        ident = f"{group}0"
        if only and ident not in only:
            continue
        rc, out, hung = run(clone, tests_of(group), TIMEOUT_S)
        results = re.findall(r"^test result: .*$", out, re.M)
        lines.append(f"BASELINE {group} (unmutated): exit {rc}; {results[-1] if results else 'no result'}")
        print(lines[-1], flush=True)
    for ident, name, path, old, new in MUTANTS:
        if only and ident not in only:
            continue
        full = os.path.join(clone, path)
        text = open(full).read()
        if text.count(old) != 1:
            lines.append(f"NOT APPLIED {ident} {name}: {text.count(old)} occurrences")
            print(lines[-1], flush=True)
            continue
        open(full, "w").write(text.replace(old, new))
        try:
            rc, out, _ = run(clone, ["cargo", "test", "--locked", "--lib", "--no-run"])
            if rc != 0:
                lines.append(f"BUILD FAILED {ident} {name} ({path})")
                lines += ["    " + l for l in out.splitlines() if l.startswith("error")][:5]
                print(lines[-1], flush=True)
                continue
            rc, out, hung = run(clone, tests_of(ident), TIMEOUT_S)
            if hung:
                head = f"KILLED (hang) {ident} {name} ({path}): the tests ran past {TIMEOUT_S} s and were killed"
                lines.append(head)
            else:
                failed = re.findall(r"^test (\S+) \.\.\. FAILED$", out, re.M)
                results = re.findall(r"^test result: .*$", out, re.M)
                verdict = "KILLED" if failed else ("SURVIVED" if rc == 0 else "KILLED (no test named)")
                head = f"{verdict} {ident} {name} ({path}): {len(failed)} failed"
                lines.append(head)
                lines += [f"    {t}" for t in failed]
                lines += [f"    {r}" for r in results[-1:]]
            print(head, flush=True)
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", path], check=True)
    status = subprocess.run(["git", "-C", clone, "status", "--porcelain"], capture_output=True, text=True).stdout
    lines.append(f"clone after: {'clean' if not status.strip() else 'DIRTY: ' + status}")
    open(out_path, "w").write("\n".join(lines) + "\n")
    print(lines[-1])


main()
