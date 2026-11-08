"""AT-HEIGHT's mutants: each rule this unit adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit. A mutant the tests fail is KILLED.

usage: python3 -I mutants.py <clone> [<mutant id>...]
The clone must be at the unit's head with no change; each mutant is restored
with `git checkout -- <file>` in the clone before the next. Prints one line per
mutant and `mutants: N run, K killed`.
"""

import os
import signal
import subprocess
import sys

sys.dont_write_bytecode = True

TESTS = [
    "tui::file_picker::tests",
    "tui::event_loop::tests::the_at_list",
    "tui::event_loop::tests::the_frame_that_gives_the_plane_back",
    "tui::event_loop::tests::the_rows_painted_are_kept",
    "tui::shell::tests::a_waiting_prompts_rows_come_out",
    "tui::shell::tests::a_frame_that_gave_the_list_no_rows",
    "tui::shell::tests::an_at_opens_the_file_list",
    "tui::shell::tests::a_query_narrows_the_list",
    "tui::shell::tests::a_page_key_moves_the_list",
    "tui::shell::tests::a_later_at_puts_the_rows",
    "tui::shell::tests::control_j_and_k_walk_the_list",
]
LIMIT_S = 900

FP = "src/tui/file_picker.rs"
SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"

MUTANTS = [
    ("ten-is-six", FP, "const MOST_FILE_ROWS: u16 = 10;", "const MOST_FILE_ROWS: u16 = 6;"),
    ("no-transcript-floor", FP, "const TRANSCRIPT_ROWS: u16 = 5;", "const TRANSCRIPT_ROWS: u16 = 0;"),
    ("grown-only", FP, "        list.max(grown)\n", "        grown\n"),
    ("list-only", FP, "        list.max(grown)\n", "        list\n"),
    ("never-none-gone", FP, "        let list = if available == 0 {\n            1\n", "        let list = if available == 0 {\n            0\n"),
    ("banner-ignored-in-height", FP, "            .saturating_add(banner_rows);\n", ";\n"),
    ("band-passes-no-banner", SH, "let banner = if banner > 0 { banner + 1 } else { 0 };", "let banner = 0;"),
    ("band-banner-without-gap", SH, "let banner = if banner > 0 { banner + 1 } else { 0 };", "let banner = banner;"),
    ("step-scrolls-six", SH, "files.step(delta, self.file_rows_painted)", "files.step(delta, file_picker::MOST_ROWS)"),
    ("step-ignores-rows", FP, "self.window_start = edge_start(self.window_start, count, self.selected, painted);", "self.window_start = edge_start(self.window_start, count, self.selected, MOST_ROWS);"),
    ("landed-band-forgets", EL, "    // And the `@` list's arrows scroll the rows this frame showed it in.\n    shell.file_picker_landed();\n", ""),
    ("restore-forgets", EL, "                    shell.file_picker_landed();\n                    failures.succeeded();", "                    failures.succeeded();"),
    ("landed-any-slot", SH, "if self.geometry.panel > 0 && matches!(self.slot(), Some(Slot::Files(_))) {", "if self.geometry.panel > 0 {"),
    ("landed-no-panel-guard", SH, "if self.geometry.panel > 0 && matches!(self.slot(), Some(Slot::Files(_))) {", "if matches!(self.slot(), Some(Slot::Files(_))) {"),
    ("reset-per-word", SH, "            self.file_picker = Some(files);\n            return;\n", "            self.file_picker = Some(files);\n            self.file_rows_painted = file_picker::MOST_ROWS;\n            return;\n"),
    ("refresh-refits-window", FP, "        self.rows = rows;\n        self.shown = shown;\n        true\n", "        self.rows = rows;\n        self.shown = shown;\n        self.window_start = edge_start(self.window_start, self.rows.len(), self.selected, MOST_ROWS);\n        true\n"),
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
    ran = killed = 0
    for mid, path, old, new in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        if text.count(old) != 1:
            print(f"BROKEN   {mid}: the text to mutate is in {path} {text.count(old)} times")
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
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
            done = subprocess.CompletedProcess(cmd, child.returncode)
            if done.returncode == 0:
                verdict = "SURVIVED"
            else:
                killed += 1
                verdict = "KILLED"
            failing = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
            if done.returncode != 0 and not failing:
                failing = ["(no test ran: " + next((l for l in out.splitlines() if l.startswith("error")), "build failed") + ")"]
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed")


if __name__ == "__main__":
    main()
