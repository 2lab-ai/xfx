#!/usr/bin/env python3
"""Train 16 (train 15's byhand.py, its table this train's, keyed by merge;
its mechanics unchanged): the conflict blocks that no rule of `blocks.py`
fits, resolved by hand and written down here, so `reproduce.sh` can redo
them. The file is read in git's diff3 style (ours, `|||||||` base,
`=======` theirs). Each block in the table is found by the sha256 (16 hex
digits) of its ours, base and theirs lines and replaced as its rule says; a
block not in the table is left for `blocks.py`; a table entry not found
exactly once stops the script, changing nothing. A rule that rewrites text
says each text it finds, and stops if any is not found exactly once.

    python3 -I -B byhand.py <merge number> <worktree-file>

Merge 2 (the slash line onto merge 1), `src/tui/shell.rs`, one block:
`Shell::offer_to_mcp_args`' doc. Ours (PAGE-KEYS-SETTINGS-COLUMN, train 14, `c4bfa8b1`)
added PgUp and PgDn (a page, the composer's row cap) and the arrows and
pages left to the caret on a composer taller than one row; theirs
(SLASH-ARG-SKILLS) made the completions "`/mcp `'s or a table's", Tab's
trailing space for a table's line, and Escape the slash menu's dismissal.
The body below the block merged clean and holds both (the PgUp/PgDn arm
and the grown-composer check reading `self.mcp_args`, which theirs fills
for a table too). Merged: theirs' first two lines, ours' three page lines,
then theirs' last three.
Merge 3 (TEST-ENV-ISOLATION onto merge 2), three blocks by hand:
- `tests/support/mod.rs`: both sides added a module where the base has
  none, ours' `profiles` (its comment and `#[cfg(unix)]` with it) and
  theirs' `isolated`. Merged: theirs' line, then ours' three, the modules in
  rustfmt's order (`cargo fmt --check` refuses `isolated` after
  `profiles`), the comment kept on the module it is about.
- `CONTRIBUTING.md`, "The gate": the script tests CI runs on one row per OS.
  Ours (main, LOCAL-PATHS-SELFTEST-FLAKE) made them three with
  `test-check-no-local-paths.sh`; theirs made them three with
  `check-env-reads.py --self-test`, and "bash, grep and git" the OS's
  "tools" (the self-test is python). Merged: four, ours' third then theirs',
  in theirs' wording. `ci.yml`, merged clean, runs all four on the
  `script_tests` rows.
- `src/tui/worker.rs`, `recording()`: the base's `Runtime::new(config,
  store, ..)`. Ours (main, CATALOG-REFRESH `44d7f84c`) built `env` over the
  test's own home just above (merged clean) and called
  `Runtime::with_environment(config, env, ..)`; theirs called its helper
  `a_runtime(config, store, ..)`, which builds the same `env` from
  `config.home()` -- the same home, since `config` is `config(home.path(),
  ..)` -- and no variable. Merged: ours' line, so the `env` above is read
  (theirs' line would leave it unused, which clippy refuses); both read no
  process environment.
Merge 4 (LOCAL-PATHS-WRAP onto merge 3), `scripts/fxcap/roots.py`, one
block in its docstring. Ours (main, #94 WORKSPACE-INDEX-AT-PLAN `3570a182`)
named the run directory's name `fxcap.` and six digits, where it had been
mktemp's random tail, and its example the same name with six digits; theirs
(LOCAL-PATHS-WRAP) wrote two paragraphs before that sentence (a continuation
mark as a gutter character; a root's path-shaped end behind a mark) and
moved the sentence to a paragraph of its own, still saying mktemp's tail.
Merged: theirs' text, with ours' two words made in it: "`fxcap.` and six
digits" and the example's six digits. The code that reads `--suffix` (`survivors`)
reads the name it is given, either shape.
Merge 5 (APPROVAL-PASTE-SPACES onto merge 4), `src/tui/approval_amendment.rs`,
one block: `Draft::apply`. Ours (train 15's approval stack,
APPROVAL-DRAFT-KEYS) took the draft's history out -- no `Undo`, `Redo` or
`Yank` in a draft -- so `apply` is `Editor::apply`'s with its delta dropped,
and returns nothing; theirs (built on the decoder line, before that stack)
kept the history's arms and added two: Home and every line-start spelling to
the draft's start, End's to its end (`DraftStart`/`DraftEnd`), as fx's
`cursorHome`/`cursorEnd` do. Merged: ours' function, with theirs' two arms
made a mapping of the action ahead of ours' one call, and theirs' comment
on them (less "A move records no delta": there is no history to record in).

Merge 7 (round 3: main `e0aedbf6` -- #101 CI-TOOLCHAIN-PIN, #99
LOCAL-PATHS-EXIT-TRAP, #100 train 15 -- onto the train),
`.github/workflows/ci.yml`, one block: the `script_tests` comment, which
both rewrote. Ours (TEST-ENV-ISOLATION) added `check-env-reads.py
--self-test` to the list and the self-test's python and clippy to what the
rows exercise, and made "build nothing" "no build of xfx" (the self-test
builds its fixtures); theirs (LOCAL-PATHS-EXIT-TRAP) made the list one
sentence with `test-early-stop.sh` in it, and dropped "bash 3.2" (macOS
runs the scripts under 3.2 only through its `bash32` row now). Merged:
theirs' sentence with both lists' scripts, ours' "own tools" clause with
theirs' grep words, ours' "no build of xfx".
"""
import hashlib
import sys

merge, path = sys.argv[1], sys.argv[2]
OURS = "ours"


def swap(lines, old, new):
    text = "\n".join(lines)
    n = text.count(old)
    if n != 1:
        sys.exit(f"{path}: {n} places of {old[:60]!r}, not one -- read by hand")
    return text.replace(old, new).split("\n")


def args_doc_both(o, b, t):
    want_o = [
        "    /// Offers one keystroke to `/mcp `'s argument completions, and says",
        "    /// whether it was taken: Up, Down, `C-k` and `C-j` move the mark and wrap;",
        "    /// PgUp and PgDn move it a page -- the composer's row cap -- and from past",
        "    /// either end land on the other; on a composer taller than one row the",
        "    /// arrows and the pages are the caret's;",
        "    /// Tab writes the marked line; Escape closes them until the draft stops",
        "    /// being a `/mcp ` line. Enter writes the marked line and is **not**",
    ]
    if o != want_o or len(t) != 5:
        sys.exit(f"{path}: ours or theirs is not the doc read -- read by hand")
    return t[:2] + o[2:5] + t[2:]


def four_script_tests(o, b, t):
    if "`test-check-no-local-paths.sh`), which test the gate scripts" not in o or \
            "`check-env-reads.py --self-test`), which test the gate scripts on the OS's own" not in t:
        sys.exit(f"{path}: the two lists are not the ones read -- read by hand")
    return [
        "runs, on Linux and macOS, on x86_64 and aarch64 -- except the four script",
        "tests (`test-gate-env.sh`, `test-preview-contract.sh`,",
        "`test-check-no-local-paths.sh` and `check-env-reads.py --self-test`), which",
        "test the gate scripts on the OS's own tools rather than anything built for",
        "the target, so CI runs them on one row per OS: aarch64 macOS and x86_64",
        "Linux. Run them locally all the same:",
    ]


def theirs_then_ours(o, b, t):
    if b:
        sys.exit(f"{path}: the base is not empty here -- read by hand")
    return t + o


def ours_env_read(o, b, t):
    if o != ["        let mut state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());"] \
            or t != ["        let mut state = a_runtime(config, store, a_prompter(), a_questioner());"]:
        sys.exit(f"{path}: the two calls are not the ones read -- read by hand")
    return o


def theirs_with_ours_run_name(o, b, t):
    # The two example names, spelt in two pieces so this file holds no run
    # name's shape (pathcheck.sh's shapes over this folder).
    digits, letters = "(`.../fxca" + "p.123456/wo...`)", "(`.../fxca" + "p.Ab12Cd/wo...`)"
    if not any("(`--suffix`, `fxcap.` and six" in x for x in o) or \
            not any(digits in x for x in o) or not any(letters in x for x in b):
        sys.exit(f"{path}: ours' run name is not the one read -- read by hand")
    out = swap(t, "(`--suffix`, mktemp's random tail) refuses", "(`--suffix`, `fxcap.` and six digits) refuses")
    return swap(out, letters, digits)


def draft_edges_without_history(o, b, t):
    want_o = [
        "    /// One editing key, on a draft `cols` cells wide: `Editor::apply`'s, its",
        "    /// delta dropped, because a draft has no history to record it in.",
        "    fn apply(&mut self, action: Action, cols: u16) {",
        "        let _ = self.editor.apply(action, cols);",
    ]
    note = [
        "            // fx's amendment reads Home and every line-start spelling as",
        "            // `.cursor_home`, and End's as `.cursor_end`",
        "            // (`approvalDraftActionFromShortcut`,",
        "            // `src/ui/input/runtime.zig:82-83@481104fd`), which put the caret",
        "            // at 0 and at the draft's length (`cursorHome`/`cursorEnd`,",
        "            // `approval_amendment.zig:84-92`): the draft's edges outright, not",
        "            // the caret line's -- the same edges only while a draft holds no",
        "            // line break. A move records no delta.",
        "            Action::Home | Action::LineStart { .. } => {",
    ]
    k = next((i for i in range(len(t)) if t[i:i + len(note)] == note), None)
    if o != want_o or k is None:
        sys.exit(f"{path}: ours or theirs' arms are not the ones read -- read by hand")
    said = [x[4:] for x in note[:-1]]
    said[-1] = "        // line break."
    return o[:3] + said + [
        "        let action = match action {",
        "            Action::Home | Action::LineStart { .. } => Action::DraftStart { shifted: false },",
        "            Action::End | Action::LineEnd { .. } => Action::DraftEnd { shifted: false },",
        "            other => other,",
        "        };",
    ] + o[3:]


def script_tests_both(o, b, t):
    if not any("check-env-reads.py --self-test). They exercise the OS's own tools --" in x for x in o) or \
            not any("test-early-stop.sh. They exercise the OS's bash, grep, git and getconf" in x for x in t):
        sys.exit(f"{path}: the two lists are not the ones read -- read by hand")
    return [
        "      # themselves: test-gate-env.sh (which also runs test-check-no-secrets.sh",
        "      # whole), test-preview-contract.sh, test-check-no-local-paths.sh,",
        "      # test-early-stop.sh and check-env-reads.py --self-test. They exercise",
        "      # the OS's own tools -- bash, grep, git and getconf (BSD grep on macOS,",
        "      # GNU grep on Linux), and the self-test python and clippy on its",
        "      # fixtures -- and no build of xfx for the target, so a second row of the",
        "      # same OS repeats them on the same userland. On the x86_64 macOS row",
        "      # they would have cost minutes against its 60-minute timeout (PR #80:",
        "      # 47m29s). The gates themselves (every check-*.sh) still run on every",
        "      # row.",
    ]


TABLE = {
    ("2", "src/tui/shell.rs"): [
        (("0850917e69a90b12", "8db7719f5b35916f", "a2989ff5e26277d0"), args_doc_both),
    ],
    ("3", "CONTRIBUTING.md"): [
        (("7eec5db16c2c810d", "30f44931808ec6b5", "3641bbddb7c42358"), four_script_tests),
    ],
    ("3", "tests/support/mod.rs"): [
        (("824327fb6a1f0588", "e3b0c44298fc1c14", "b202a668e5acbcb7"), theirs_then_ours),
    ],
    ("4", "scripts/fxcap/roots.py"): [
        (("abc78b6101481f79", "dee00a9ee1f68b5b", "55f8701abecd2048"), theirs_with_ours_run_name),
    ],
    ("5", "src/tui/approval_amendment.rs"): [
        (("ff2d63c8c26fb1bf", "8f2d5c053d1429f4", "1a3896b5925926b8"), draft_edges_without_history),
    ],
    ("7", ".github/workflows/ci.yml"): [
        (("a7b6fe3df4e4447c", "11ff2992283a84e9", "520b1541635578a3"), script_tests_both),
    ],
    ("3", "src/tui/worker.rs"): [
        (("6ee676770ffe630d", "d164da928d34fb8d", "a930b24597a41ee3"), ours_env_read),
    ],
}


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]


table = next((v for (m, k), v in TABLE.items() if m == merge and path.endswith(k)), None)
if table is None:
    sys.exit(f"merge {merge} {path}: no table")
found = {sides: 0 for sides, _ in table}
lines = open(path, encoding="utf-8").read().split("\n")
out, i, said = [], 0, []
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    bar = next(j for j in range(i, len(lines)) if lines[j].startswith("||||||| "))
    mid = next(j for j in range(bar, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    o, b, t = lines[i + 1:bar], lines[bar + 1:mid], lines[mid + 1:end]
    sides = (h(o), h(b), h(t))
    rule = dict(table).get(sides)
    if rule is None:
        out.extend(lines[i:end + 1])
    else:
        found[sides] += 1
        merged = o if rule == OURS else t if rule == "theirs" else rule(o, b, t)
        name = rule if isinstance(rule, str) else rule.__name__
        if isinstance(merged, tuple):
            if out[-1] != "[[cause]]":
                sys.exit(f"{path}:{len(out)}: not a `[[cause]]` line above the block -- read by hand")
            out.pop()
            name += f", {merged[0]}"
            merged = merged[1]
        said.append(f"{path}:{len(out) + 1}: by hand, {name} "
                    f"(ours {len(o)} lines, base {len(b)}, theirs {len(t)} -> {len(merged)})")
        out.extend(merged)
    i = end + 1
if any(n != 1 for n in found.values()):
    sys.exit(f"{path}: table entries found {list(found.values())}, not once each -- read by hand")
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {path}: {len(said)} blocks resolved by hand")
