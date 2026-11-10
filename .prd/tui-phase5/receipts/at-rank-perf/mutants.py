"""AT-RANK-PERF's mutants: the pruning, the letter mask, the facts an entry
keeps from the walk, and the narrowed dynamic program, each broken one at a
time in a throwaway clone (`git clone --no-local`), and the modules that read
the `@` list run against it with a time limit, in their own process group.
A mutant the tests fail is KILLED; one that does not build is NO-BUILD and
not counted as killed. (AT-RANK's runner, `../at-rank/mutants.py`, with this
unit's mutants; round 3 adds four.)

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

FOLDER = "                folder: !(top.exact || top.name),\n"
CROSSING = "                crossing: !(top.exact || (top.name && top.quality > max_crossing)),\n"
LIMIT = "    if !exact && limit.is_some_and(|limit| lower.len() > limit) {\n"
MAX_NAME = "    let max_name = SCORE_MATCH * m + BONUS_BOUNDARY_DELIMITER * (m + 1) + NAME_BONUS;\n"
REPLACE = "            kept[worst] = (score, index);\n"
ADMITS = "    entry.mask & NON_ASCII != 0 || (query.ascii.is_some() && entry.mask & query.mask == query.mask)\n"
ADOPT = "                    let entries = std::mem::take(&mut walk.published);\n"
ADDED = "            entries.push(Candidate::new(path, candidate.kind));\n"
RESET = "            s.scores[cell..cell + 4].fill(INVALID);\n"
ROW0 = """            let bonus = bonus_at(path, offsets, j);
            if row == 0 {
                let slot = run_slot(bonus);
                s.scores[cell + slot] = SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER;
                s.starts[cell + slot] = s.columns[entry];
                continue;
            }
"""


def edits(*pairs):
    def mutate(text):
        for old, new in pairs:
            if text.count(old) != 1:
                return None, f"the text to mutate is there {text.count(old)} times: {old.strip()[:60]}"
            text = text.replace(old, new)
        return text, None
    return mutate


MUTANTS = [
    # pruning that drops a winner
    ("folder-cut-whenever-full", edits((FOLDER, "                folder: false,\n"))),
    ("crossing-cut-whenever-full", edits((CROSSING, "                crossing: false,\n"))),
    ("crossing-cut-at-its-ceiling", edits((CROSSING, CROSSING.replace("> max_crossing", ">= max_crossing")))),
    ("length-limit-cuts-an-equal-length", edits((LIMIT, LIMIT.replace("> limit", ">= limit")))),
    ("length-limit-cuts-an-exact-match", edits((LIMIT, "    if limit.is_some_and(|limit| lower.len() > limit) {\n"))),
    ("best-name-without-its-bonus", edits((MAX_NAME, MAX_NAME.replace(" + NAME_BONUS", "")))),
    ("worst-not-found-again", edits((REPLACE, REPLACE + "            continue;\n"))),
    # a mask that admits a non-match, or refuses a match
    ("mask-any-shared-letter", edits((ADMITS, ADMITS.replace(
        "entry.mask & query.mask == query.mask", "(entry.mask & query.mask != 0 || query.mask == 0)")))),
    ("mask-gone", edits((ADMITS, "    entry.mask & NON_ASCII != 0 || query.ascii.is_some()\n"))),
    ("mask-non-ascii-bit-ignored", edits((ADMITS, ADMITS.replace("entry.mask & NON_ASCII != 0 || ", "")))),
    # facts that are not the entry's own path's
    ("stale-facts-after-a-refresh", edits((ADOPT,
        "                    let mut entries = std::mem::take(&mut walk.published);\n"
        "                    if let Some(old) = &self.index {\n"
        "                        for (new, old) in entries.iter_mut().zip(old.iter()) {\n"
        "                            (new.lower, new.name, new.mask) = (old.lower.clone(), old.name, old.mask);\n"
        "                        }\n"
        "                    }\n"))),
    ("stale-facts-on-an-added-directory", edits((ADDED, "            entries.push(Candidate { path, ..candidate });\n"))),
    ("name-start-not-kept", edits(("            name: basename_start(&path),\n", "            name: 0,\n"))),
    # the narrowed scratch
    ("cell-reset-gone", edits((RESET, ""))),
    ("cell-reset-after-the-first-row", edits((RESET + ROW0, ROW0 + RESET))),
    # round 3 (AT-RANK-PERF r2): each cut switched off -- no row changes, so
    # only the work count can see it -- and the ASCII length guard at 2048
    ("folder-cut-never", edits((FOLDER, "                folder: true,\n"))),
    ("crossing-cut-never", edits((CROSSING, "                crossing: true,\n"))),
    ("length-limit-never", edits(
        ("                limit = Some(0);\n", "                limit = None;\n"),
        ("                limit = Some(entries[at].path.len());\n", "                limit = None;\n"))),
    ("ascii-guard-at-2048", edits((
        "    if lower.is_empty() || lower.len() > MOST_PATH_BYTES {\n",
        "    if lower.is_empty() || lower.len() >= MOST_PATH_BYTES {\n"))),
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
