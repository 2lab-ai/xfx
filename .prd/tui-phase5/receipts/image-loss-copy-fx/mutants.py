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
CAPTURE = "scripts/fxcap/capture.sh"
APP = "src/app.rs"
W = "src/tui/worker.rs"

# The tests of each rule, by target.
RESUME = {
    "lib": [
        "session::store::tests::a_prompt_images_copy_gone_or_not_a_file",
        "session::store::tests::a_prompt_images_copy_the_resume_cannot_stat",
        "tui::worker::tests::the_picker_cannot_resume",
    ],
    "sessions": ["ask_resume_of_a_session_a_prompt_images_copy_is_gone"],
    "tui": ["a_resume_of_a_session_a_prompt_images_copy_is_gone_from"],
}
REQUEST = {
    "lib": [
        "session::store::tests::a_resumed_prompt_images_copy_is_read_when_the_history_is_built",
        "session::store::tests::a_prompt_image_never_read_back_goes_without",
        "session::store::tests::each_request_sends_a_copy",
        "tui::worker::tests::a_resumed_prompt_images_copy_is_read_at_each_request",
        "tui::worker::tests::a_resumed_copy_is_checked_again_at_each_request",
        "tui::worker::tests::a_resumed_copy_is_held_back_past_each_limit",
        "tui::worker::tests::a_copy_a_request_cannot_carry",
        "tui::worker::tests::a_prompt_image_never_read_back_is_held_back",
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
    "        if let Some(error) = read_snapshots(&dir, &mut state) {\n"
    "            return Err(SessionError::Unreadable {\n"
    "                id: id.as_str().to_string(),\n"
    "                error,\n"
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
        [(STORE, REFUSAL, REFUSAL.replace("read_snapshots(&dir, &mut state) {", "read_snapshots(&dir, &mut state).filter(|_| false) {"))],
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
                + "        if let Some(error) = refused {\n"
                "            return Err(SessionError::Unreadable {\n"
                "                id: id.as_str().to_string(),\n"
                "                error,\n"
                "            });\n"
                "        }\n",
            ),
        ],
        RESUME,
    ),
    # Round 2: a stat that fails resumes the session; only a copy not there, or not a file, refuses it.
    "stat-failure-resumed": (
        [(STORE, '                    Err(_) => Some("Io"),\n', "                    Err(_) => None,\n")],
        {"lib": ["session::store::tests::a_prompt_images_copy_the_resume_cannot_stat"]},
    ),
    # Round 3: a failed stat refused in the gone copy's words, not upstream's `Io`.
    "stat-failure-named-invalid": (
        [(STORE, '                    Err(_) => Some("Io"),\n', '                    Err(_) => Some("InvalidSessionFormat"),\n')],
        {
            "lib": ["session::store::tests::a_prompt_images_copy_the_resume_cannot_stat"],
            "sessions": ["ask_resume_of_a_session_a_prompt_images_copy_is_gone"],
        },
    ),
    # Round 3: the images folder followed when it is a link.
    "folder-followed": (
        [(STORE, "OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW", "OFlags::RDONLY | OFlags::DIRECTORY")],
        {"lib": ["session::store::tests::a_prompt_images_copy_the_resume_cannot_stat"]},
    ),
    # Round 3: the folder not opened for reading, the copy `lstat`ed through its path (round 2's check):
    # a folder that can be searched but not read, and a linked one, let the resume go on.
    "folder-not-opened": (
        [
            (STORE, "rustix::fs::open(&folder, flags | OFlags::CLOEXEC, Mode::empty())", 'rustix::fs::open("/", flags | OFlags::CLOEXEC, Mode::empty())'),
            (
                STORE,
                "rustix::fs::statat(folder, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)",
                "rustix::fs::statat(folder, &dir.join(IMAGES_DIR).join(&name), rustix::fs::AtFlags::SYMLINK_NOFOLLOW)",
            ),
        ],
        {"lib": ["session::store::tests::a_prompt_images_copy_the_resume_cannot_stat"]},
    ),
    # Round 2: the read moved back to the resume -- a copy whole then is held in memory, never read again.
    "read-at-resume": (
        [
            (
                STORE,
                "            image.snapshot = Snapshot::Saved(Arc::from(folder.join(name)));\n",
                "            let path = folder.join(name);\n"
                "            image.snapshot = match read_snapshot(&path, &image.recorded.sha256, image.recorded.bytes) {\n"
                "                Ok(bytes) => Snapshot::Loaded { bytes, path: Some(Arc::from(path)) },\n"
                "                Err(_) => Snapshot::Saved(Arc::from(path)),\n"
                "            };\n",
            )
        ],
        REQUEST,
    ),
    # Round 3 (S2): the history reads a copy whole, to the 20 MiB limit, where upstream reads its header
    # and its size.
    "build-reads-past-a-request": (
        [(STORE, "read_from(file, length, PAST_A_REQUEST)", "read_from(file, length, crate::image::MAX_IMAGE_BYTES + 1)")],
        {"lib": ["session::store::tests::a_resumed_prompt_images_copy_is_read_when_the_history_is_built"]},
    ),
    # Round 2: a copy past the limit held back without its detail (the brief's "ImageTooLarge without
    # its detail"; upstream's build never reaches ImageTooLarge, its note carries the detail): no byte
    # read of a copy past what one request carries, so the note says the dimensions could not be
    # verified, not the type, the pixels and the limits.
    "oversized-without-detail": (
        [
            (
                STORE,
                ".and_then(|(file, length)| read_from(file, length, PAST_A_REQUEST))",
                ".and_then(|(file, length)| if length >= PAST_A_REQUEST { Ok(Vec::new()) } else { read_from(file, length, PAST_A_REQUEST) })",
            )
        ],
        {"lib": ["tui::worker::tests::a_copy_a_request_cannot_carry", "tui::worker::tests::a_resumed_copy_is_held_back_past_each_limit"]},
    ),
    # The request going on past a changed copy, with its bytes.
    "request-goes-on": (
        [(STORE, "                    Ok(_) => failed = Some(COPY_CHANGED),\n", "                    Ok(bytes) => *data = Arc::from(bytes),\n")],
        REQUEST,
    ),
    # Round 3 (MUST 1): a copy that cannot be opened again sends what the history read -- the catch-all
    # arm of round 2 -- with no note.
    "catch-all-sends": (
        [
            (STORE, "                    notes.push_str(&crate::image::withheld_note(\n", "                    let _ = (&mut notes, crate::image::withheld_note(\n"),
            (STORE, "                        Some(saved),\n                    ));\n                    return false;\n", "                        Some(saved),\n                    ));\n                    return true;\n"),
        ],
        REQUEST,
    ),
    # Round 4 (MUST 1): the read stops at the size the open found, so a copy grown since is sent as
    # its checked prefix.
    "growth-accepted": (
        [(STORE, "read_from(file, length, crate::image::MAX_IMAGE_BYTES + 1)", "read_from(file, length, length)")],
        REQUEST,
    ),
    # Round 4 follow-up: a read shorter than the recorded size is sent without its digest checked.
    "short-read-accepted": (
        [
            (
                STORE,
                "                        if bytes.len() as u64 == copy.size\n"
                "                            && crate::image::digest(&bytes) == copy.sha256 =>\n",
                "                        if (bytes.len() as u64) < copy.size\n"
                "                            || bytes.len() as u64 == copy.size\n"
                "                                && crate::image::digest(&bytes) == copy.sha256 =>\n",
            )
        ],
        {"lib": ["session::store::tests::each_request_sends_a_copy"]},
    ),
    # Round 4: a copy past the limit when it is read fails as changed, not as upstream's
    # `ImageTooLarge`.
    "too-large-as-changed": (
        [(STORE, '                        failed = Some("ImageTooLarge");\n', "                        failed = Some(COPY_CHANGED);\n")],
        REQUEST,
    ),
    # Round 4 (S1): a read that fails lets the request go: with the history's bytes taken first,
    # it sends none of them.
    "read-failure-goes-on": (
        [(STORE, '                    Err(_) => failed = Some("ReadFailed"),\n', "                    Err(_) => {}\n")],
        REQUEST,
    ),
    # Round 4 (MUST 2): `homemode` changes a file's mode, a hard link's included.
    "homemode-on-a-file": (
        [(CAPTURE, '        [[ -d $at ]] || { echo "homemode takes a folder, not ~/$dest" >&2; exit 2; }\n', "")],
        {"capture": ["homemode"]},
    ),
    # Round 3 (MUST 1): the bytes just checked not swapped in; the history's go.
    "swap-in-deleted": (
        [(STORE, "                        *data = Arc::from(bytes);\n", "                        let _ = bytes;\n")],
        REQUEST,
    ),
    # The TUI's check gone: the turn's provider sees no copies.
    "request-goes-on-tui": (
        [(W, "        copies: replay_copies,\n", "        copies: {\n            let _ = replay_copies;\n            Vec::new()\n        },\n")],
        {"lib": REQUEST["lib"]},
    ),
    # `ask` saying the TUI's words in place of upstream's `ask`'s.
    "ask-name-not-copied": (
        [(APP, "            (Event::Error { .. }, Some(name)) => self.inner.emit(&Event::Error {\n", "            (Event::Error { .. }, Some(name)) if false => self.inner.emit(&Event::Error {\n")],
        {"sessions": ["ask_resume_whose_prompt_images_copy_changed"]},
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
        [(STORE, "                        prepend_note(&note, output),\n                        Vec::new(),\n", "                        prepend_note(&note, output),\n                        carried,\n")],
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
    # The capture harness's own tests (`test-capture.sh`), by name.
    command = (
        ["python3", "-I", "scripts/fxcap/test_capture.py", *[f"-k{test}" for test in tests]]
        if target == "capture"
        else ["cargo", "test", "--locked", *which, "--", *tests]
    )
    child = subprocess.Popen(
        command,
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
                    elif line.startswith(("FAIL: ", "ERROR: ", "AssertionError")):
                        print(f"    {line[:200]}")
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
