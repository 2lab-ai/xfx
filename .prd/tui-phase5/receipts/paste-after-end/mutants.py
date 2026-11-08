#!/usr/bin/env python3
"""PASTE-AFTER-END's mutants, each on a throwaway `git clone --no-local` of
the code commit, never on the worktree: one textual change, the TUI's lib
tests (`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), the failing tests counted, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

A mutant is killed when at least one test fails, and killed (hang) when a
test run passes TIMEOUT_S, its process group killed then. With a list, only
those mutants run (1-based). Each reverts or half-reverts one of the unit's
rules: the bytes behind the end marker are never input (a trailing CR that
submits is mutant 1); the paste is kept whatever followed it; the warning
is said, only when something followed, in fx's tone and words, after the
paste's own outcome, on the review plane's status row; and a delivery is
every read up to the drain, not one read.
"""
import os
import re
import signal
import subprocess
import sys

TIMEOUT_S = 900
PTY = [
    "a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing",
    "an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too",
]

MUTANTS = [
    ("the bytes behind the marker are decoded: a trailing CR submits", "src/tui/input.rs",
     "        if let Some(behind) = self.behind.as_mut() {\n"
     "            *behind = behind.saturating_add(1);\n"
     "            return;\n"
     "        }\n",
     ""),
    ("an ESC behind the marker is decoded (the rest counted)", "src/tui/input.rs",
     "        if let Some(behind) = self.behind.as_mut() {\n",
     "        if let Some(behind) = self.behind.as_mut().filter(|_| byte != 0x1b) {\n"),
    ("the count behind the marker is one, whatever followed", "src/tui/input.rs",
     "            *behind = behind.saturating_add(1);\n",
     "            *behind = 1;\n"),
    ("a paste with input behind it never settles", "src/tui/input.rs",
     "            Some(dropped) => Delivery::Pasted { dropped },\n",
     "            Some(0) => Delivery::Pasted { dropped: 0 },\n            Some(_) => Delivery::Quiet,\n"),
    ("c1db919's rule: a paste with input behind it is not applied", "src/tui/shell.rs",
     "            Delivery::Pasted { dropped } => {\n"
     "                self.consume(vec![Input::Action(Action::PasteEnd)], Instant::now());\n",
     "            Delivery::Pasted { dropped } => {\n"
     "                if dropped == 0 {\n"
     "                    self.consume(vec![Input::Action(Action::PasteEnd)], Instant::now());\n"
     "                }\n"),
    ("no warning", "src/tui/shell.rs",
     "                if dropped > 0 {\n                    self.ignored_behind_paste();\n                }\n",
     "                let _ = dropped;\n"),
    ("the warning with nothing behind the paste too", "src/tui/shell.rs",
     "                if dropped > 0 {\n                    self.ignored_behind_paste();\n",
     "                if dropped < usize::MAX {\n                    self.ignored_behind_paste();\n"),
    ("the warning before the paste is settled", "src/tui/shell.rs",
     "                self.consume(vec![Input::Action(Action::PasteEnd)], Instant::now());\n"
     "                if dropped > 0 {\n                    self.ignored_behind_paste();\n                }\n",
     "                if dropped > 0 {\n                    self.ignored_behind_paste();\n                }\n"
     "                self.consume(vec![Input::Action(Action::PasteEnd)], Instant::now());\n"),
    ("the warning is an error (c1db919's tone)", "src/tui/shell.rs",
     '            None => self.announce(notice::Notice::warning("input", PASTE_SUFFIX)),\n',
     '            None => self.announce(notice::Notice::error("input", PASTE_SUFFIX)),\n'),
    ("c1db919's sentence", "src/tui/shell.rs",
     'const PASTE_SUFFIX: &str = "Input that arrived right after the paste was ignored.";\n',
     'const PASTE_SUFFIX: &str = "Paste was not applied because extra input followed its end marker.";\n'),
    ("on the review plane the warning is not said", "src/tui/shell.rs",
     "            Some(screen) => screen.set_status(PASTE_SUFFIX.to_string()),\n"
     '            None => self.announce(notice::Notice::warning("input", PASTE_SUFFIX)),\n',
     "            Some(_) => {}\n"
     '            None => self.announce(notice::Notice::warning("input", PASTE_SUFFIX)),\n'),
    ("a delivery is one read: the loop settles after every read", "src/tui/event_loop.rs",
     "            Ok(read) => shell.route_read(&buffer[..read]),\n",
     "            Ok(read) => {\n                shell.route_read(&buffer[..read]);\n                shell.end_delivery();\n            }\n"),
]


def run(cmd, cwd, env):
    """`cmd` in its own process group; (exit code or None on a timeout, output)."""
    proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=TIMEOUT_S)
        return proc.returncode, out.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.communicate()
        return None, ""


def failed(output):
    return sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.M)))


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    chosen = None
    if len(sys.argv) > 3:
        chosen = {int(n) for n in sys.argv[3].split(",")}
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE")}
    head = subprocess.run(["git", "-C", clone, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True, env=env).stdout.strip()
    lines = [f"clone at {head}; per test run timeout {TIMEOUT_S} s"]
    for n, (what, path, old, new) in enumerate(MUTANTS, 1):
        if chosen and n not in chosen:
            continue
        file = os.path.join(clone, path)
        text = open(file, encoding="utf-8", newline="").read()
        if text.count(old) != 1:
            lines.append(f"{n:2}. NOT APPLIED ({text.count(old)} matches): {what}")
            continue
        open(file, "w", encoding="utf-8", newline="").write(text.replace(old, new))
        try:
            verdicts = []
            for cmd in (["cargo", "test", "--locked", "--lib", "tui::"],
                        ["cargo", "test", "--locked", "--test", "tui", "--"] + PTY):
                code, output = run(cmd, clone, env)
                if code is None:
                    verdicts.append("hang")
                elif "error[E" in output or "could not compile" in output:
                    verdicts.append("did not compile")
                else:
                    verdicts.append(failed(output) if code else [])
            names = [v for v in verdicts if isinstance(v, list)]
            killed = [name for v in names for name in v]
            if "did not compile" in verdicts:
                verdict = "DID NOT COMPILE"
            elif killed:
                verdict = f"killed by {len(killed)}: " + ", ".join(killed[:6]) + (" ..." if len(killed) > 6 else "")
            elif "hang" in verdicts:
                verdict = "killed (hang)"
            else:
                verdict = "SURVIVED"
            lines.append(f"{n:2}. {verdict} -- {what} ({path})")
        finally:
            open(file, "w", encoding="utf-8", newline="").write(text)
        print(lines[-1], flush=True)
    with open(out_path, "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
