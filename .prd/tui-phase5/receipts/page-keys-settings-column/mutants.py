"""PAGE-KEYS-SETTINGS-COLUMN's mutants, each on a `git clone --no-local`
throwaway of the head and never on the worktree. Per list (the /settings
menu, the /model column, /mcp's argument completions): the page key back to
nothing, a page of one row, and a page taken round the list where fx sends it
to the far end; and for the two lists this unit gave them, `C-j`/`C-k` back
to nothing; and the settings list's moves sent to the menu under it. For
each, the clone's tree is reset, the one edit made (exactly one match after
its anchor, or the run stops), and the unit's tests run; a mutant is killed
when a test fails.

usage: python3 -I mutants.py <clone> <out.txt>
"""

import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True

TESTS = ["at_24_rows"]
ROUND_SELECTED = (
    "self.selected = usize::try_from((i64::try_from(self.selected % count).unwrap() + delta)"
    ".rem_euclid(i64::try_from(count).unwrap())).unwrap();"
)
ROUND_INDEX = (
    "*index = usize::try_from((i64::try_from(*index % count).unwrap() + delta)"
    ".rem_euclid(i64::try_from(count).unwrap())).unwrap();"
)
PAGE_ARMS = "Action::PageUp => -page,\n                    Action::PageDown => page,"
ONE_ARMS = "Action::PageUp => -1,\n                    Action::PageDown => 1,"

# (name, file, anchor or None, old, new): `old` is replaced at its first match
# after `anchor`, which must be the only match in that fn (or the file).
MUTANTS = [
    (
        "settings:no-page-key",
        "src/tui/settings.rs",
        None,
        "            Action::PageDown => Some(Self::Page(true)),\n            Action::PageUp => Some(Self::Page(false)),\n",
        "",
    ),
    (
        "settings:page-of-one",
        "src/tui/shell.rs",
        "fn offer_to_settings(",
        "SettingsAction::Page(down) => Some(if down { page } else { -page }),",
        "SettingsAction::Page(down) => Some(if down { 1 } else { -1 }),",
    ),
    (
        "settings:round",
        "src/tui/settings.rs",
        "pub(crate) fn step_by(",
        "self.selected = advanced(self.selected, count, delta);",
        ROUND_SELECTED,
    ),
    (
        "settings:no-row-keys",
        "src/tui/settings.rs",
        None,
        "            Action::ControlJ => Some(Self::Row(true)),\n            Action::ControlK => Some(Self::Row(false)),\n",
        "",
    ),
    (
        "settings-list:moves-the-menu",
        "src/tui/shell.rs",
        "fn offer_to_settings(",
        "_ if listing && delta.is_some() => {",
        "SettingsAction::Up | SettingsAction::Down if listing => {",
    ),
    (
        "column:no-page-key",
        "src/tui/shell.rs",
        "fn offer_to_column(",
        "                        | Action::PageUp\n                        | Action::PageDown\n",
        "",
    ),
    ("column:page-of-one", "src/tui/shell.rs", "fn offer_to_column(", PAGE_ARMS, ONE_ARMS),
    (
        "column:round",
        "src/tui/model_column.rs",
        "fn advance(",
        "*index = advanced(*index, count, delta);",
        ROUND_INDEX,
    ),
    (
        "column:no-row-keys",
        "src/tui/shell.rs",
        "fn offer_to_column(",
        "                        | Action::ControlK\n                        | Action::ControlJ\n",
        "",
    ),
    (
        "mcp:no-page-key",
        "src/tui/shell.rs",
        "fn offer_to_mcp_args(",
        "            | Action::PageUp\n            | Action::PageDown => {",
        " => {",
    ),
    ("mcp:page-of-one", "src/tui/shell.rs", "fn offer_to_mcp_args(", PAGE_ARMS, ONE_ARMS),
    (
        "mcp:round",
        "src/tui/mcp_args.rs",
        "pub(crate) fn step_by(",
        "self.selected = advanced(self.selected, count, delta);",
        ROUND_SELECTED,
    ),
]


def run(cmd, cwd, timeout=None):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def mutate(text, name, anchor, old, new):
    start = 0
    end = len(text)
    if anchor is not None:
        assert text.count(anchor) == 1, (name, "anchor", text.count(anchor))
        start = text.index(anchor)
        # The anchor's fn: up to the next `fn ` at its indentation or deeper.
        nxt = re.search(r"\n    (?:pub(?:\(crate\)|\(super\))? )?fn |\n(?:pub(?:\(crate\)|\(super\))? )?fn ", text[start + len(anchor) :])
        end = start + len(anchor) + (nxt.start() if nxt else len(text))
    region = text[start:end]
    assert region.count(old) == 1, (name, region.count(old))
    at = start + region.index(old)
    return text[:at] + new + text[at + len(old) :]


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    lines = []
    for name, path, anchor, old, new in MUTANTS:
        run(["git", "checkout", "--", "src"], clone)
        full = f"{clone}/{path}"
        text = open(full, encoding="utf-8").read()
        open(full, "w", encoding="utf-8").write(mutate(text, name, anchor, old, new))
        diff = run(["git", "diff", "--stat"], clone).stdout.strip().splitlines()[-1]
        start = time.time()
        res = run(["cargo", "test", "--locked", "--lib", "--", *TESTS], clone, timeout=1800)
        failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", res.stdout, re.M)))
        built = "test result:" in res.stdout
        verdict = "KILLED" if failed else ("SURVIVED" if built else "NO-BUILD")
        lines.append(f"{name}: {verdict} ({len(failed)} failed, {time.time() - start:.0f}s; {diff})")
        for f in failed:
            lines.append(f"    {f}")
        if not built:
            err = res.stderr.strip().splitlines()
            lines.append("    " + (err[-1] if err else "(no output)"))
        print(lines[-1 - len(failed)] if built else lines[-2], flush=True)
    run(["git", "checkout", "--", "src"], clone)
    head = run(["git", "rev-parse", "--short", "HEAD"], clone).stdout.strip()
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(f"# mutants over the head {head} on a --no-local clone; tests: {' '.join(TESTS)}\n")
        f.write("\n".join(lines) + "\n")
    killed = sum(1 for l in lines if ": KILLED" in l)
    total = sum(1 for l in lines if not l.startswith("    "))
    print(f"killed {killed}/{total}")


if __name__ == "__main__":
    main()
