#!/usr/bin/env python3
"""PASTE-GAP round 3's mutants, each on a throwaway `git clone --no-local` of
the code commit, never on the worktree: one textual change, the TUI's lib
tests (`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), every failing test named, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

`GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command.
Mutants 1-6 revert round 3's rules: a lone ESC is released by the quiet (1)
and said (2); the read cap says what is owed (3) and settles nothing (4); the
repeat says what holds the quarantine (5, 6). Mutants 7-12 are round 2's core
rules again on round 3's code: a start marker in the quarantine begins a paste
(7), no quiet ends it with a marker read past its first byte (8) or a second
paste being dropped (9), the markers are followed behind the end marker (10),
the empty checks do not restart the quiet (11), and fx's boundary (12).
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
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval",
]

WHOLE = "        let whole = !self.dropping_paste && self.following <= 1;\n"

MUTANTS = [
    ("round 2: a lone ESC is held like a longer marker", "src/tui/input.rs",
     WHOLE, "        let whole = !self.dropping_paste && self.following == 0;\n"),
    ("a lone ESC the quiet drops is never said", "src/tui/input.rs",
     "                    if quarantine.said_at.is_none() {\n"
     "                        self.owed = quarantine.unsaid.saturating_add(1);\n"
     "                    }\n",
     ""),
    ("the read cap says nothing", "src/tui/event_loop.rs",
     "            shell.still_at(Instant::now());\n", ""),
    ("the read cap settles and ends as an empty check does", "src/tui/input.rs",
     "        self.owed_warning(now)\n    }\n\n    /// The warning the quarantine owes",
     "        self.end_delivery(now)\n    }\n\n    /// The warning the quarantine owes"),
    ("an open dropped paste's repeat says \"until it stops\"", "src/tui/input.rs",
     "        if self.dropping_paste {\n            Hold::Paste\n",
     "        if false {\n            Hold::Paste\n"),
    ("a half-read marker's repeat says \"until it stops\"", "src/tui/input.rs",
     "        } else if self.following > 1 {\n            Hold::Marker\n",
     "        } else if false {\n            Hold::Marker\n"),
    ("round 1: a start marker in the quarantine is dropped with the rest", "src/tui/input.rs",
     'const PASTE_START: &[u8] = b"\\x1b[200~";\n',
     'const PASTE_START: &[u8] = b"\\x00\\x00\\x00\\x00\\x00\\x00";\n'),
    ("the quarantine ends with a marker read past its first byte", "src/tui/input.rs",
     WHOLE, "        let whole = !self.dropping_paste;\n"),
    ("the quarantine ends with a second paste still being dropped", "src/tui/input.rs",
     WHOLE, "        let whole = self.following <= 1;\n"),
    ("behind the end marker the markers are not followed", "src/tui/input.rs",
     "            // Counted, and followed: a second paste begun here is being\n"
     "            // dropped until its own end marker, whenever that comes.\n"
     "            self.follow(byte);\n",
     ""),
    ("every empty check restarts the quiet", "src/tui/input.rs",
     "            Some(_) => {}\n            // Something was read",
     "            Some(_) => quarantine.quiet_since = Some(now),\n            // Something was read"),
    ("fx's boundary: no quarantine after a paste", "src/tui/input.rs",
     "            self.quarantine = Some(Quarantine {\n"
     "                unsaid: 0,\n"
     "                said_at: (dropped > 0).then_some(now),\n"
     "                quiet_since: Some(now),\n"
     "            });\n",
     ""),
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
            print(lines[-1], flush=True)
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
                verdict = f"killed by {len(killed)}: " + ", ".join(killed)
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
