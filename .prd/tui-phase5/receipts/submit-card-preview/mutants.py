#!/usr/bin/env python3
"""SUBMIT-CARD-PREVIEW's mutants: each one line of the unit taken out or
changed, on a throwaway clone, and the unit's tests run against it.

    python3 -I -B mutants.py <repo> <head> <clone-dir> <target-dir>

<clone-dir> is made with `git clone --no-local <repo>` and checked out at
<head>; nothing is written into <repo>. Each mutant edits one file of the
clone (its text must occur exactly once), runs the six tests below with a
600 s limit (CARGO_TARGET_DIR=<target-dir>), and puts the file back. A
mutant is killed when the run fails; the unmutated clone runs first and must
pass.
"""
import os
import subprocess
import sys

repo, head, clone, target = sys.argv[1:5]
TESTS = [
    "a_prompt_taller_than_the_room_under_the_document",
    "an_exit_before_the_preview_lands",
    "a_resize_before_the_preview_lands",
    "a_cards_preview_is_upstreams",
    "a_preview_takes_rows_from_the_band_s_top",
    "a_card_is_held_from_the_document_while_its_preview_is_up",
]
SHELL, LOOP, LAYOUT = "src/tui/shell.rs", "src/tui/event_loop.rs", "src/tui/layout.rs"
MUTANTS = [
    ("room: no floor of one row", SHELL,
     ".saturating_sub(document)\n            .max(1);", ".saturating_sub(document);"),
    ("room: not cut to the frame's band", SHELL,
     "Some(usize::from(tail.min(running.saturating_sub(3))))", "Some(usize::from(tail))"),
    ("room: fx's footer five rows, not four", SHELL,
     "const UPSTREAM_FOOTER_ROWS: u16 = 4;", "const UPSTREAM_FOOTER_ROWS: u16 = 5;"),
    ("room: measured after the draft went", SHELL,
     "if let Some(room) = room {\n                        self.preview_card(",
     "if let Some(room) = self.preview_room() {\n                        let _ = room;\n                        self.preview_card("),
    ("room: a document still owed rows", SHELL,
     "if self.transcript.owes()\n            || self.departing.is_some()", "if self.departing.is_some()"),
    ("room: under a row that is not blank", SHELL,
     "            || !self.transcript.ends_blank()\n", ""),
    ("room: under a fork's line", SHELL,
     "            || self.raw_gap\n", ""),
    ("the band: no preview rows", SHELL,
     ".and_then(|rows| geometry.previewed(rows))", ".and_then(|_| geometry.previewed(0))"),
    ("the preview: the oldest rows, not the newest", SHELL,
     "let shown = rows.split_off(rows.len() - room);", "rows.truncate(room);\n        let shown = rows;"),
    ("the hold: the held card counted as owed", SHELL,
     "        !self.previewing()\n            && (", "        (true)\n            && ("),
    ("the hold: the card written in the preview's frame", SHELL,
     "        if self.previewing() {\n            return None;\n        }\n", ""),
    ("the landing: the preview never let go", SHELL,
     "    pub(crate) fn band_landed(&mut self) {\n        self.landed_bottom = Some(self.geometry.content_bottom);\n        self.preview_landed();\n",
     "    pub(crate) fn band_landed(&mut self) {\n        self.landed_bottom = Some(self.geometry.content_bottom);\n"),
    ("the landing: the band not solved again", SHELL,
     "        if self.preview.take().is_some() || self.previewing() {\n            self.refit();",
     "        if self.preview.take().is_some() || self.previewing() {"),
    ("the exit: no frame for a held card", LOOP,
     "(shell.paced_backlog() > 0 || shell.previewing())", "shell.paced_backlog() > 0"),
    ("the exit: the held card not let go", SHELL,
     "        self.drop_preview();\n        self.finish_saves();", "        self.finish_saves();"),
    ("a resize: the preview kept", SHELL,
     "        // written whole by the frame this owes ([`Self::preview`]).\n        self.preview = None;\n",
     "        // written whole by the frame this owes ([`Self::preview`]).\n"),
    ("the geometry: the last document row taken", LAYOUT,
     "            .checked_sub(anchored.gap + rows + 1)\n            .filter(|bottom| *bottom > 0)?;",
     "            .checked_sub(anchored.gap + rows + 1)?;"),
]


def run(*args, **kw):
    return subprocess.run(list(args), **kw)


if not os.path.isdir(clone):
    run("git", "clone", "-q", "--no-local", repo, clone, check=True)
run("git", "-C", clone, "checkout", "-q", "--detach", head, check=True)
env = dict(os.environ, CARGO_TARGET_DIR=target)
for var in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(var, None)


def tests():
    try:
        done = run("cargo", "test", "--locked", "--lib", "--", *TESTS, cwd=clone, env=env,
                   capture_output=True, text=True, timeout=600)
    except subprocess.TimeoutExpired:
        return "timed out", []
    out = done.stdout + done.stderr
    failing = sorted({line.split()[1].split("::")[-1] for line in out.split("\n")
                      if line.startswith("test ") and line.endswith("FAILED")})
    if done.returncode != 0 and not failing and "error[" in out:
        return "did not build", []
    return ("passed" if done.returncode == 0 else "failed"), failing


verdict, _ = tests()
print("unmutated: %s" % verdict)
if verdict != "passed":
    sys.exit(1)
survived = 0
for name, path, old, new in MUTANTS:
    full = os.path.join(clone, path)
    text = open(full).read()
    if text.count(old) != 1:
        print("SKIPPED  %s: its text occurs %d times" % (name, text.count(old)))
        survived += 1
        continue
    open(full, "w").write(text.replace(old, new))
    try:
        verdict, failing = tests()
    finally:
        run("git", "-C", clone, "checkout", "-q", "--", path, check=True)
    killed = verdict in ("failed", "timed out", "did not build")
    if not killed:
        survived += 1
    print("%-8s %s (%s): %s" % ("killed" if killed else "SURVIVED", name, path, verdict +
          (" -- " + ", ".join(failing) if failing else "")))
print("%d of %d mutants survived" % (survived, len(MUTANTS)))
sys.exit(1 if survived else 0)
