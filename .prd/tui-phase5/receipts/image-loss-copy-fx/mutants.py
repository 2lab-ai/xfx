#!/usr/bin/env python3
"""IMAGE-LOSS-COPY-FX's mutants, each on a throwaway clone (`git clone --no-local`), never on
the worktree.

    python3 -I -B mutants.py <clone> <label> [<mutant>...]

For each mutant: the clone's tracked files are reset (`git checkout -- .`), the mutant's
replacements are applied (each must match exactly once, or the mutant is refused), and each of
its test targets runs the named tests under a 900 s limit (a process group killed at the
deadline). A target is `killed` when its tests run and fail, `SURVIVED` when they pass,
`timeout` at the deadline, and `build-failed` when the mutant does not compile (no test ran:
not a kill). The clone is reset after each mutant and at the end. Prints one line per mutant
and target, then each failing test and its panic line.
"""
import os
import signal
import subprocess
import sys

STORE = "src/session/store.rs"
APP = "src/app.rs"
W = "src/tui/worker.rs"

# The tests of each rule, by target.
RESUME = {
    "lib": [
        "session::store::tests::a_prompt_images_copy_gone_or_not_a_file",
        "tui::worker::tests::the_picker_cannot_resume",
    ],
    "sessions": ["ask_resume_of_a_session_a_prompt_images_copy_is_gone"],
    "tui": ["a_resume_of_a_session_a_prompt_images_copy_is_gone_from"],
}
REQUEST = {
    "lib": [
        "session::store::tests::a_prompt_images_copy_that_cannot_go",
        "session::store::tests::a_prompt_image_never_read_back",
        "tui::worker::tests::a_resumed_prompt_image_whose_copy_changed",
        "tui::worker::tests::a_prompt_image_never_read_back_fails",
    ],
    "sessions": ["ask_resume_whose_prompt_images_copy_changed"],
}
TOOL = {
    "lib": [
        "session::store::tests::a_tool_images_lost_copy",
        "session::store::tests::a_tool_image_never_read_back",
        "tui::worker::tests::a_tool_image_that_cannot_go",
    ],
    "sessions": ["ask_resume_never_adopts_a_model_with_a_control_character_or_prints_one"],
}

REPLAY = (
    "    let replay = conversation\n"
    "        .recorder\n"
    "        .state()\n"
    "        .history_messages(state.config.provider.wire(), &model);\n"
)
LOST_TOOL_IMAGE = (
    "conversation.recorder.state().turns.iter().flat_map(|turn| &turn.steps).any(|step| "
    "matches!(step, crate::session::TurnStep::ToolResult { images, .. } "
    "if images.iter().any(|image| !matches!(image.snapshot, crate::session::Snapshot::Loaded { .. }))))"
)
# Right after `ask` has opened its session and before its sink is made.
OPENED_END = "    // On a terminal the answer arrives inert: the approval question is asked\n"
REFUSAL = (
    "        if read_snapshots(&dir, &mut state) {\n"
    "            return Err(SessionError::Unreadable {\n"
    "                id: id.as_str().to_string(),\n"
    "            });\n"
    "        }\n"
)
TRUNCATED = "            file.sync_data().map_err(|err| io_error(&log_path, err))?;\n        }\n"

MUTANTS = {
    # A notice line kept, on the TUI's screen: a lost tool image said under `session`.
    "notice-kept-tui": (
        [
            (
                W,
                REPLAY,
                REPLAY
                + f"    if {LOST_TOOL_IMAGE} {{\n"
                "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(super::notice::Notice::warning(\n"
                "            \"session\",\n"
                "            \"an image `read_file` returned could not be read back, so it was not sent again\",\n"
                "        ))).await;\n"
                "    }\n",
            )
        ],
        {"lib": TOOL["lib"]},
    ),
    # A notice line kept, on `ask`'s stderr.
    "notice-kept-ask": (
        [
            (
                APP,
                OPENED_END,
                "    if opened.history.iter().flat_map(|message| message.content.iter()).any(|part| "
                "matches!(part, crate::gateway::protocol::ContentPart::ToolResult { output, .. } "
                "if output.starts_with(\"[Stored tool image\"))) {\n"
                "        writeln!(stderr, \"xfx: an image a tool returned could not be read back, so it was not sent again\")?;\n"
                "    }\n"
                + OPENED_END,
            )
        ],
        {"sessions": TOOL["sessions"]},
    ),
    # The resume going on past a lost prompt image.
    "resume-goes-on": (
        [(STORE, REFUSAL, REFUSAL.replace("if read_snapshots(&dir, &mut state) {", "if read_snapshots(&dir, &mut state) && false {"))],
        RESUME,
    ),
    # A refused resume touching the session file: refused only after the log's tail is cut.
    "refusal-after-truncate": (
        [
            (STORE, REFUSAL, "        let refused = read_snapshots(&dir, &mut state);\n"),
            (
                STORE,
                TRUNCATED,
                TRUNCATED
                + "        if refused {\n"
                "            return Err(SessionError::Unreadable {\n"
                "                id: id.as_str().to_string(),\n"
                "            });\n"
                "        }\n",
            ),
        ],
        RESUME,
    ),
    # The request going on past a prompt image it cannot send (the store says nothing).
    "request-goes-on": (
        [(STORE, "            .find_map(prompt_image_failure)\n", "            .find_map(prompt_image_failure)\n            .filter(|_| false)\n")],
        REQUEST,
    ),
    # The TUI's check gone, the store's kept.
    "request-goes-on-tui": (
        [(W, "        if let Some(error) = unsendable {\n", "        if let Some(error) = unsendable.filter(|_| false) {\n")],
        {"lib": REQUEST["lib"]},
    ),
    # The tool-image text missing: the note not put in front.
    "tool-note-missing": (
        [(STORE, "    format!(\"{note}{}\", &output[..keep])\n", "    let _ = (note, keep);\n    output.to_string()\n")],
        TOOL,
    ),
    # The tool-image text changed: another error's name for a copy that is not there.
    "tool-note-changed": (
        [(STORE, "            Self::Missing => \"ResultHandleNotFound\",\n", "            Self::Missing => \"BlobNotFound\",\n")],
        TOOL,
    ),
    # The note said, and the result's other images still sent.
    "tool-images-kept": (
        [(STORE, "                        Vec::new(),\n                    ),\n", "                        carried,\n                    ),\n")],
        TOOL,
    ),
}


def apply(clone, edits):
    for path, old, new in edits:
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as handle:
            text = handle.read()
        count = text.count(old)
        if count != 1:
            raise SystemExit(f"{path}: the mutant's text matches {count} times")
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(text.replace(old, new))


def run(clone, target, tests):
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        env.pop(name, None)
    which = ["--lib"] if target == "lib" else ["--test", target]
    child = subprocess.Popen(
        ["cargo", "test", "--locked", *which, "--", *tests],
        cwd=clone,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        text=True,
    )
    try:
        out, _ = child.communicate(timeout=900)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        out, _ = child.communicate()
        return "timeout", out
    if child.returncode != 0 and ("error[E" in out or "could not compile" in out):
        return "build-failed", out
    if "running 0 tests" in out and "test result: ok" in out and out.count("running ") == out.count("running 0 tests"):
        return "no-test-ran", out
    return ("SURVIVED" if child.returncode == 0 else "killed"), out


def main():
    clone, label, names = sys.argv[1], sys.argv[2], sys.argv[3:]
    subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
    print(f"# clone at {subprocess.check_output(['git', '-C', clone, 'rev-parse', '--short', 'HEAD'], text=True).strip()}", flush=True)
    for name in names or list(MUTANTS):
        edits, targets = MUTANTS[name]
        apply(clone, edits)
        try:
            for target, tests in targets.items():
                verdict, out = run(clone, target, tests)
                print(f"{label} {name} [{target}]: {verdict}", flush=True)
                lines = out.splitlines()
                for at, line in enumerate(lines):
                    if line.startswith("test ") and line.endswith("FAILED"):
                        print(f"    {line}")
                    elif "panicked at" in line:
                        print(f"    {line.split('panicked at', 1)[1].strip()}")
                        if at + 1 < len(lines):
                            print(f"      {lines[at + 1][:200]}")
                    elif line.startswith("error[E") and verdict == "build-failed":
                        print(f"    {line}")
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
    changed = subprocess.check_output(["git", "-C", clone, "status", "--porcelain", "--untracked-files=no"], text=True)
    print(f"# changed files in the clone after the run: {len(changed.splitlines())}")


if __name__ == "__main__":
    main()
