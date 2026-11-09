#!/usr/bin/env python3
"""PASTE-GAP's mutants, each on a throwaway `git clone --no-local` of the code
commit, never on the worktree: one textual change, the TUI's lib tests
(`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), the failing tests counted, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

A mutant is killed when at least one test fails, and killed (hang) when a test
run passes TIMEOUT_S, its process group killed then. With a list, only those
mutants run (1-based). `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are
unset for every command. Each reverts or half-reverts one of the unit's rules:
every paste is followed by a quarantine (mutant 1 is fx's boundary; mutant 6
is the brief's narrower trigger, a paste whose delivery dropped bytes); a
quarantined byte is counted and never decoded; quiet is two empty checks
PASTE_QUIET apart with nothing read between, not one check and not the clock;
and the warning is owed once a paste.
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

QUARANTINE = (
    "            self.quarantine = Some(Quarantine {\n"
    "                dropped: 0,\n"
    "                said: dropped > 0,\n"
    "                quiet_since: Some(now),\n"
    "            });\n"
)

MUTANTS = [
    ("fx's boundary: no quarantine after a paste", "src/tui/input.rs", QUARANTINE, ""),
    ("a quarantined byte is decoded", "src/tui/input.rs",
     "        if let Some(quarantine) = self.quarantine.as_mut() {\n"
     "            quarantine.dropped = quarantine.dropped.saturating_add(1);\n"
     "            quarantine.quiet_since = None;\n"
     "            return;\n"
     "        }\n",
     ""),
    ("a byte inside the quarantine does not restart the quiet", "src/tui/input.rs",
     "            quarantine.quiet_since = None;\n            return;\n",
     "            return;\n"),
    ("one empty check is quiet enough", "src/tui/input.rs",
     "            Some(since) if now.saturating_duration_since(since) >= Self::PASTE_QUIET => {\n",
     "            Some(since) if now.saturating_duration_since(since) >= Duration::ZERO => {\n"),
    ("quiet is the clock since the last byte, not two empty checks", "src/tui/input.rs",
     "            quarantine.quiet_since = None;\n            return;\n",
     "            quarantine.quiet_since = Some(now);\n            return;\n"),
    ("a quarantine only after a paste whose delivery dropped bytes", "src/tui/input.rs",
     "            self.quarantine = Some(Quarantine {\n",
     "            self.quarantine = (dropped > 0).then_some(Quarantine {\n"),
    ("input dropped in a quarantine is never said", "src/tui/shell.rs",
     "            Delivery::Ignored { .. } => self.ignored_behind_paste(),\n",
     "            Delivery::Ignored { .. } => {}\n"),
    ("a quarantine says its warning at every check that dropped something", "src/tui/input.rs",
     "            quarantine.said = true;\n",
     ""),
    ("a paste whose delivery was said is said again by its quarantine", "src/tui/input.rs",
     "                said: dropped > 0,\n",
     "                said: false,\n"),
    # PASTE-AFTER-END's mutant 1, kept: the bytes behind the marker are decoded.
    ("the bytes behind the marker are decoded: a trailing CR submits", "src/tui/input.rs",
     "        if let Some(behind) = self.behind.as_mut() {\n"
     "            *behind = behind.saturating_add(1);\n"
     "            return;\n"
     "        }\n",
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
