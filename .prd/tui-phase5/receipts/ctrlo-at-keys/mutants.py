"""CTRLO-AT-KEYS' mutants: each key's routing under Ctrl-O broken one at a
time in a throwaway clone (`git clone --no-local`), and the unit's modules
run against it with a time limit, in their own process group. A mutant the
tests fail is KILLED; one that does not build is NO-BUILD and not counted as
killed. (AT-LAUNCH-WALK's runner, `../at-launch-walk/mutants.py`, with this
unit's modules and mutants, and a failed build kept apart from a kill.)

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

# The modules whose cases read the `@` list or the full transcript's keys.
TESTS = [
    "tui::shell",
    "tui::event_loop",
    "tui::full_transcript",
    "tui::file_picker",
    "tui::file_index",
]
LIMIT_S = 900

SH = "src/tui/shell.rs"
FT = "src/tui/full_transcript.rs"

HAND_ON = "        if transcript && matches!(action, Action::ControlJ | Action::ControlK) {\n"
TAB_FIRST = "            Action::Tab if !eligible => return true,\n"
RETRY = "                    None if files.shown() == file_picker::Shown::Unavailable => {\n"


def once(old, new):
    def mutate(text):
        if text.count(old) != 1:
            return None, f"the text to mutate is there {text.count(old)} times"
        return text.replace(old, new), None
    return mutate


def both(*pairs):
    def mutate(text):
        for old, new in pairs:
            if text.count(old) != 1:
                return None, f"the text to mutate is there {text.count(old)} times"
            text = text.replace(old, new)
        return text, None
    return mutate


MUTANTS = [
    # C-j under Ctrl-O spent again, C-k still handed on
    ("cj-routing-reverted", SH, once(HAND_ON, HAND_ON.replace("Action::ControlJ | ", ""))),
    # C-k under Ctrl-O spent again, C-j still handed on
    ("ck-routing-reverted", SH, once(HAND_ON, HAND_ON.replace(" | Action::ControlK", ""))),
    # both handed on on the composer's plane too: C-j/C-k no longer move the bar
    ("cj-ck-handed-on-always", SH, once(HAND_ON, HAND_ON.replace("transcript && ", ""))),
    # handed on whenever the list is not eligible, not only under the transcript
    ("cj-ck-handed-on-whenever-ineligible", SH, once(HAND_ON, HAND_ON.replace("transcript && ", "!eligible && "))),
    # Tab's eligibility check moved after the re-read: an unreadable list is
    # read again first
    ("tab-check-after-the-reread", SH, once(
        TAB_FIRST,
        "            Action::Tab if !eligible && files.shown() != file_picker::Shown::Unavailable => return true,\n",
    )),
    # Tab's eligibility check gone: no early return, an ungated retry
    ("tab-check-gone", SH, once(TAB_FIRST, "")),
    # Tab as the base had it: the row resolved first, the retry arm gated
    ("tab-base-order", SH, both(
        (TAB_FIRST, ""),
        (RETRY, RETRY.replace("None if files", "None if eligible && files")),
    )),
    # Enter under Ctrl-O taken by the list (AT-HEIGHT round 3's rule)
    ("enter-taken-under-the-transcript", SH, once(
        "            Action::Submit if !eligible => return false,\n",
        "            Action::Submit if false => return false,\n",
    )),
    # a letter under Ctrl-O spent while a list is open
    ("letter-spent-under-the-transcript", SH, once(
        "    fn offer_to_files(&mut self, event: &Input) -> bool {\n"
        "        let Input::Action(action) = event else {\n"
        "            return false;\n",
        "    fn offer_to_files(&mut self, event: &Input) -> bool {\n"
        "        let Input::Action(action) = event else {\n"
        "            return self.full.is_open() && self.file_picker.is_some() && matches!(event, Input::Text(_));\n",
    )),
    # Escape not the transcript's: it reaches the list, which closes
    ("escape-not-the-transcripts", FT, once(
        "            Action::ToggleTranscript | Action::Escape | Action::Cancel => Reply::Close,\n",
        "            Action::ToggleTranscript | Action::Cancel => Reply::Close,\n",
    )),
    # Down not the transcript's: it reaches the list, which spends it
    ("down-not-the-transcripts", FT, once(
        "            Action::Down => moved(false, Some(ARROW_ROWS)),\n",
        "            Action::Down => Reply::Unbound,\n",
    )),
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
    for mid, path, mutate in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        new, why = mutate(text)
        if new is None:
            print(f"BROKEN   {mid}: {why} ({path})", flush=True)
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
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed, {broken} did not build")


if __name__ == "__main__":
    main()
