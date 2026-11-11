"""AT-FOLDERS' mutants: each rule this unit adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit, in their own process group. A mutant the tests fail is
KILLED. (The runner is AT-HEIGHT's, `../at-height/mutants.py`, with this
unit's tests and mutants.)

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
    "tui::file_index::tests",
    "tui::event_loop::tests::the_frame_of_a_keystroke_that_lists_a_folder",
    "tui::event_loop::tests::a_key_the_list_hands_on_mid_resize",
    "tui::event_loop::tests::a_key_spent_mid_resize",
    "tui::shell::tests::a_later_at_puts_the_rows",
    "tui::shell::tests::a_directory_choice",
    "tui::shell::tests::enter_on_a_folder_row",
    "tui::shell::tests::tab_on_the_folders_own_row",
    "tui::shell::tests::a_folder_with_a_space",
    "tui::shell::tests::a_keystroke_that_lists_a_folder",
    "tui::shell::tests::only_a_folder_listing_holds",
    "tui::shell::tests::after_a_slow_folder",
    "tui::shell::tests::enter_puts_the_path",
    "tui::shell::tests::an_unmatched_mention",
]
LIMIT_S = 900

FP = "src/tui/file_picker.rs"
FI = "src/tui/file_index.rs"
SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"

MUTANTS = [
    # I4: Enter selects a folder, Tab opens it
    ("select-is-open", FP, "let opens = directory && action == FolderAction::Open;", "let opens = directory;"),
    ("open-is-select", FP, "let opens = directory && action == FolderAction::Open;", "let opens = false;"),
    ("select-no-slash", FP, 'encode(&format!("{}/", found.path), query.quoted, false, indexed)', "encode(&found.path, query.quoted, false, indexed)"),
    ("select-quote-left-open", FP, 'encode(&format!("{}/", found.path), query.quoted, false, indexed)', "encode(&found.path, query.quoted, true, indexed)"),
    ("select-no-space", FP, "let adds = !opens && !reuses;", "let adds = !directory && !reuses;"),
    ("select-caret-before-space", FP, "usize::from(!opens && reuses)", "usize::from(!directory && reuses)"),
    ("tab-selects", SH, "Some(found) => self.complete_file(&found, file_picker::FolderAction::Open),", "Some(found) => self.complete_file(&found, file_picker::FolderAction::Select),"),
    ("enter-opens", SH, "self.complete_file(&found, file_picker::FolderAction::Select);", "self.complete_file(&found, file_picker::FolderAction::Open);"),
    # I4: a listed folder is row 1
    ("no-folder-row", FI, "let folder = listed_folder(explicit).map(|folder| Found {", "let folder = listed_folder(explicit).filter(|_| false).map(|folder| Found {"),
    ("folder-row-for-a-name", FI, "if !explicit.name.is_empty() || explicit.prefix.len() < 2 {", "if explicit.prefix.len() < 2 {"),
    ("folder-row-for-the-root", FI, "if !explicit.name.is_empty() || explicit.prefix.len() < 2 {", "if !explicit.name.is_empty() {"),
    ("folder-row-for-a-double-slash", FI, "if folder.ends_with('/') || !is_terminal_safe(folder) {", "if !is_terminal_safe(folder) {"),
    ("folder-row-unsafe", FI, "if folder.ends_with('/') || !is_terminal_safe(folder) {", "if folder.ends_with('/') {"),
    ("folder-takes-no-slot", FI, "ranked.truncate(MOST_RESULTS - usize::from(folder.is_some()));", "ranked.truncate(MOST_RESULTS);"),
    # I5: the frame hold
    ("loop-never-holds", EL, "    if shell.hold_frame_for_file_listing(now) {\n        return Ok(());\n    }\n", ""),
    ("shell-never-holds", SH, "        self.listing_wait.hold(in_flight, now)\n", "        let _ = self.listing_wait.hold(in_flight, now);\n        false\n"),
    ("hold-ignores-indexed", SH, "let in_flight = !files.indexed() && files.shown() == file_picker::Shown::Loading;", "let in_flight = files.shown() == file_picker::Shown::Loading;"),
    ("hold-ignores-loading", SH, "let in_flight = !files.indexed() && files.shown() == file_picker::Shown::Loading;", "let in_flight = !files.indexed();"),
    ("hold-takes-no-rows", SH, "    pub(crate) fn hold_frame_for_file_listing(&mut self, now: Instant) -> bool {\n        self.settle_files();\n", "    pub(crate) fn hold_frame_for_file_listing(&mut self, now: Instant) -> bool {\n"),
    ("closed-list-keeps-wait", SH, "            self.listing_wait = file_picker::ListingWait::default();\n", ""),
    ("keystrokes-extend-wait", FP, "        if self.started.is_none() {\n            self.started = Some(now);\n        }\n", "        self.started = Some(now);\n"),
    ("slow-never-recorded", FP, "self.last_outlasted_hold = !within_hold(started, now);", "self.last_outlasted_hold = false;"),
    ("slow-is-forever", FP, "self.last_outlasted_hold = !within_hold(started, now);", "self.last_outlasted_hold |= !within_hold(started, now);"),
    ("active-ignores-slow", FP, "!self.last_outlasted_hold && self.measuring(now)", "self.measuring(now)"),
    ("measuring-is-active", FP, "        self.started\n            .is_some_and(|started| within_hold(started, now))\n", "        !self.last_outlasted_hold && self.started.is_some_and(|started| within_hold(started, now))\n"),
    ("hold-inclusive", FP, ".is_some_and(|waited| waited < LISTING_HOLD)", ".is_some_and(|waited| waited <= LISTING_HOLD)"),
    ("backwards-clock-extends", FP, "    now.checked_duration_since(started)\n        .is_some_and(|waited| waited < LISTING_HOLD)\n", "    now.saturating_duration_since(started) < LISTING_HOLD\n"),
    ("hold-is-16-ms", FP, "std::time::Duration::from_millis(8);", "std::time::Duration::from_millis(16);"),
    ("wait-is-the-tick", EL, "    if shell.file_listing_hold_pending(now) {\n        LISTING_POLL.min(TICK)\n", "    if false && shell.file_listing_hold_pending(now) {\n        LISTING_POLL.min(TICK)\n"),
    ("pending-is-active", SH, "        self.listing_wait.measuring(now)\n    }\n", "        self.listing_wait.active(now)\n    }\n"),
    # AT-HEIGHT's leftover
    ("distrust-asks-nothing", SH, "            files.distrust();\n            self.render.request(Reason::Modal);\n        }\n", "            files.distrust();\n        }\n"),
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
