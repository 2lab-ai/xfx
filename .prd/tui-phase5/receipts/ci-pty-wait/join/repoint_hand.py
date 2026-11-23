#!/usr/bin/env python3
"""The join's re-points the content check cannot see: a bare `:N` on a line
after the reference whose file it continues (refs_content.py reads a bare
`:N` on the reference's own line only). Found by refs_bare.py (this folder's;
the README's `:11516`, `:14576` and capfile.py's `:15860-15862`) and by
reading every bare `:N` in the README (`:11501`, whose nearest reference is
another file's, and `:14599`). Each entry: the citing file and line, the
token as written and as re-pointed, the cited file and what the new number
names there at the merge (931418d4), checked before it is written.

    python3 -I -B repoint_hand.py <repo> [--apply]
Prints one line per entry; numbers only. Prints no path.
"""
import subprocess
import sys

repo = sys.argv[1]
README = ".prd/tui-phase5/receipts/ci-pty-wait/README.md"
HAND = [
    (README, 87, "`:11516`", "`:11696`", "src/tui/shell.rs", "return Keyed::NotYet;"),
    (README, 97, "`:11501`", "`:11681`", "src/tui/shell.rs", "fn keyed(&self) -> Keyed {"),
    (README, 203, "`:14576`", "`:14694`", "tests/tui.rs",
     "fn found_in_sandbox(root: &std::path::Path, searched: &[&str]) -> Found {"),
    (README, 208, "`:14599`", "`:14717`", "tests/tui.rs",
     "fn a_skills_sandbox_path_never_holds_what_its_case_searches() {"),
    ("scripts/fxcap/capfile.py", 184, ":15860-15862", ":15896-15898", "src/tui/shell.rs",
     "if self.owner != ScreenOwner::Primary {"),
]
bad = 0
for citing, n, old, new, cited, text in HAND:
    merged = subprocess.run(["git", "-C", repo, "show", f"931418d4:{cited}"],
                            capture_output=True, text=True, check=True).stdout.split("\n")
    first = int(new.strip("`:").split("-")[0])
    names = merged[first - 1].strip() == text
    lines = open(f"{repo}/{citing}", encoding="utf-8").read().split("\n")
    holds = lines[n - 1].count(old) == 1
    print(f"{citing}:{n} {old} -> {new} ({cited}:{first} {'names' if names else 'DOES NOT name'} {text!r}; "
          f"{'the line holds it once' if holds else 'NOT ON THE LINE ONCE'})")
    bad += not (names and holds)
    if "--apply" in sys.argv and names and holds:
        lines[n - 1] = lines[n - 1].replace(old, new)
        open(f"{repo}/{citing}", "w", encoding="utf-8").write("\n".join(lines))
sys.exit(1 if bad else 0)
