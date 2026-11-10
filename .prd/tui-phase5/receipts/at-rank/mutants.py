"""AT-RANK's mutants: each term of fx's placement score, each tier and tie of
its order, the row cap and the cell budget broken one at a time in a
throwaway clone (`git clone --no-local`), and the modules that read the `@`
list run against it with a time limit, in their own process group. A mutant
the tests fail is KILLED; one that does not build is NO-BUILD and not
counted as killed. (CTRLO-AT-KEYS' runner, `../ctrlo-at-keys/mutants.py`,
with this unit's modules and mutants.)

usage: python3 -I mutants.py <clone> [<mutant id>...]
The clone must be at the unit's head with no change; each mutant is restored
with `git checkout -- <file>` in the clone before the next. Prints one line per
mutant and `mutants: N run, K killed, B did not build`.
"""

import os
import signal
import subprocess
import sys

sys.dont_write_bytecode = True

TESTS = ["tui::file_index", "tui::file_picker", "tui::shell"]
LIMIT_S = 900
FI = "src/tui/file_index.rs"

GAP_LINE = "            total += SCORE_MATCH + bonus + GAP_START + (gap - 1) * GAP_EXTENSION;\n"
SHIFT = "                    let shifted = value + s.columns[next_gap] as i32;\n"
DP_GAP = "                let score = gap - j as i32 - 1 + SCORE_MATCH + bonus;\n"
DIAG = '            if diag < prev.end && s.columns[diag] + 1 == j {\n                for (from, run) in RUN_BONUS.into_iter().enumerate() {\n                    let (value, start) = (s.scores[diag * 4 + from], s.starts[diag * 4 + from]);\n                    if value != INVALID {\n                        let (step, run) = consecutive_bonus(bonus, run);\n                        s.offer(\n                            cell + run_slot(run),\n                            value + SCORE_MATCH + step,\n                            start,\n                            diag,\n                            from,\n                        );\n                    }\n                }\n            }\n'
GAP = '            if gap != INVALID {\n                // A gap of `j - p - 1`: `GAP_START`, then `GAP_EXTENSION`\n                // for each position past its first (`:1676-1685`).\n                let score = gap - j as i32 - 1 + SCORE_MATCH + bonus;\n                s.offer(\n                    cell + run_slot(bonus),\n                    score,\n                    gap_start,\n                    gap_entry,\n                    gap_slot,\n                );\n            }\n'


def edits(*pairs):
    def mutate(text):
        for old, new in pairs:
            if text.count(old) != 1:
                return None, f"the text to mutate is there {text.count(old)} times: {old.strip()[:60]}"
            text = text.replace(old, new)
        return text, None
    return mutate


MUTANTS = [
    # the score's terms, each dropped (`scoreAlignment`, `bonusFor`, `consecutiveBonus`)
    ("per-letter-dropped", edits(
        ("const SCORE_MATCH: i32 = 16;\n", "const SCORE_MATCH: i32 = 0;\n"),
        ("const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2;\n", "const BONUS_BOUNDARY: i32 = 8;\n"),
        ("const BONUS_NON_WORD: i32 = SCORE_MATCH / 2;\n", "const BONUS_NON_WORD: i32 = 8;\n"))),
    ("first-letter-not-doubled", edits(
        ("const FIRST_CHAR_MULTIPLIER: i32 = 2;\n", "const FIRST_CHAR_MULTIPLIER: i32 = 1;\n"))),
    ("after-slash-bonus-dropped", edits(
        ("            Delimiter => return BONUS_BOUNDARY_DELIMITER,\n", "            Delimiter => return 0,\n"))),
    ("word-start-bonus-dropped", edits(
        ("            White => return BONUS_BOUNDARY_WHITE,\n", "            White => return 0,\n"),
        ("            NonWord => return BONUS_BOUNDARY,\n", "            NonWord => return 0,\n"))),
    ("camel-digit-bonus-dropped", edits(
        ("        return BONUS_CAMEL_123;\n", "        return 0;\n"))),
    ("punctuation-bonus-dropped", edits(
        ("        NonWord | Delimiter => BONUS_NON_WORD,\n", "        NonWord | Delimiter => 0,\n"))),
    ("run-floor-dropped", edits(
        ("    (bonus.max(run.max(BONUS_CONSECUTIVE)), run)\n", "    (bonus.max(run), run)\n"))),
    ("gap-cost-dropped", edits(
        (GAP_LINE, "            total += SCORE_MATCH + bonus;\n"),
        (SHIFT, "                    let shifted = value;\n"),
        (DP_GAP, "                let score = gap + SCORE_MATCH + bonus;\n"))),
    ("gap-extension-dropped", edits(
        (GAP_LINE, "            total += SCORE_MATCH + bonus + GAP_START;\n"),
        (SHIFT, "                    let shifted = value;\n"),
        (DP_GAP, "                let score = gap + SCORE_MATCH + bonus + GAP_START;\n"))),
    # the placement (`choosePlacement`, `exactFit`)
    ("name-bonus-off", edits(("const NAME_BONUS: i32 = 4;\n", "const NAME_BONUS: i32 = 0;\n"))),
    ("folder-placement-off", edits(
        ("    if base > 0 && query.len() > 1 && can_win {\n", "    if false && base > 0 && query.len() > 1 && can_win {\n"))),
    ("exact-dot-rule-dropped", edits(
        ("        || (name.len() > query.len() && name.starts_with(query) && name[query.len()] == '.');\n",
         "        || false;\n"))),
    # the order (`scoreBetter`, `rankedCandidateBetter`, `betterCandidate`), each step reversed
    ("exact-reversed", edits(("        .exact\n        .cmp(&left.exact)\n", "        .exact\n        .cmp(&right.exact)\n"))),
    ("name-reversed", edits((".then(right.name.cmp(&left.name))", ".then(left.name.cmp(&right.name))"))),
    ("quality-reversed", edits((".then(right.quality.cmp(&left.quality))", ".then(left.quality.cmp(&right.quality))"))),
    ("first-reversed", edits((".then(left.first.cmp(&right.first))", ".then(right.first.cmp(&left.first))"))),
    ("shorter-path-reversed", edits((".then(left.1.len().cmp(&right.1.len()))", ".then(right.1.len().cmp(&left.1.len()))"))),
    ("bytes-reversed", edits(("left.1.as_bytes().cmp(right.1.as_bytes())", "right.1.as_bytes().cmp(left.1.as_bytes())"))),
    ("earlier-start-tie-reversed", edits(
        ("    score > best || (score == best && start < best_start)\n", "    score > best || (score == best && start > best_start)\n"))),
    # the row cap and the cell budget
    ("cap-31", edits(("pub(crate) const MOST_RESULTS: usize = 32;\n", "pub(crate) const MOST_RESULTS: usize = 31;\n"))),
    ("cap-33", edits(("pub(crate) const MOST_RESULTS: usize = 32;\n", "pub(crate) const MOST_RESULTS: usize = 33;\n"))),
    ("cells-4095", edits(("const MOST_ALIGN_CELLS: usize = 4096;\n", "const MOST_ALIGN_CELLS: usize = 4095;\n"))),
    ("cells-4097", edits(("const MOST_ALIGN_CELLS: usize = 4096;\n", "const MOST_ALIGN_CELLS: usize = 4097;\n"))),
    ("greedy-fallback-gone", edits(("const MOST_ALIGN_CELLS: usize = 4096;\n", "const MOST_ALIGN_CELLS: usize = usize::MAX;\n"))),
    # round 3 (AT-RANK r2): the repeated search for the bold letters, and the
    # order the dynamic program keeps among equal candidates
    ("spans-no-rerun", edits(
        ("    if align_best(\n        &s.text,\n        path.as_bytes(),\n        &s.offsets,\n        range_start,\n",
         "    if false && align_best(\n        &s.text,\n        path.as_bytes(),\n        &s.offsets,\n        range_start,\n"))),
    ("dp-gap-before-diagonal", edits((DIAG + GAP, GAP + DIAG))),
    ("final-pick-ge", edits(
        ("        if value != INVALID && better_candidate(value, start, best, best_start) {\n",
         "        if value != INVALID && (value > best || (value == best && start <= best_start)) {\n"))),
]


def run(clone, args, **kw):
    return subprocess.run(args, cwd=clone, **kw)


def main():
    clone = os.path.realpath(sys.argv[1])
    wanted = set(sys.argv[2:])
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    status = run(clone, ["git", "status", "--porcelain"], capture_output=True, text=True, env=env)
    if status.stdout.strip():
        sys.exit(f"the clone is not clean:\n{status.stdout}")
    head = run(clone, ["git", "rev-parse", "--short=8", "HEAD"], capture_output=True, text=True, env=env).stdout.strip()
    print(f"# clone at {head}; tests: cargo test --locked --lib -- {' '.join(TESTS)}; limit {LIMIT_S} s each", flush=True)
    ran = killed = broken = 0
    for mid, mutate in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, FI)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        new, why = mutate(text)
        if new is None:
            print(f"BROKEN   {mid}: {why}", flush=True)
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(new)
        ran += 1
        try:
            cmd = ["cargo", "test", "--locked", "--lib", "--", *TESTS]
            # Its own process group, so a run past the limit is stopped with
            # every process it started, and nothing else.
            child = subprocess.Popen(
                cmd, cwd=clone, env=env, text=True, start_new_session=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            try:
                out, _ = child.communicate(timeout=LIMIT_S)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.communicate()
                raise
            failing = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
            summary = next((l for l in out.splitlines() if l.startswith("test result:")), "")
            if child.returncode == 0:
                verdict = "SURVIVED"
            elif not failing and not summary:
                broken += 1
                verdict = "NO-BUILD"
                failing = [next((l for l in out.splitlines() if l.startswith("error")), "build failed")]
            else:
                killed += 1
                verdict = "KILLED"
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}  [{summary}]", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", FI], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed, {broken} did not build")


if __name__ == "__main__":
    main()
