#!/usr/bin/env python3
"""Train 12, merge 4 (INPUT-ROW-LIMIT): the semantic conflict git's line
merge cannot see, fixed in the merge commit so its tree builds -- train 11's
semantic.py, its fix list this merge's.

INPUT-ROW-LIMIT, on base 32afd307, wrote
`at_41_rows_a_page_of_the_model_menu_is_19_models` (src/tui/shell.rs), which
builds a `CatalogLoaded` with an `Instant` due; CATALOG-REFRESH (train 11,
on main) made every such due a `CatalogDue` on the catalog's clock (E0308).
Rewritten as the tree already spells a load's due in every other test of
that module: `CatalogDue::at(catalog_now() + crate::provider::model::REFRESH_INTERVAL)`.

    python3 -I -B semantic.py <worktree>
    python3 -I -B semantic.py --reverse <worktree>   # takes it out again

Each replacement must match exactly the number of times given, or nothing
is written. `--reverse` is for the check from both ends (merge-checks.sh).
"""
import sys

reverse = sys.argv[1] == "--reverse"
root = sys.argv[-1]
FIXES = [
    ("src/tui/shell.rs",
     "        shell.shell.apply(UiEvent::CatalogLoaded {\n"
     "            due: std::time::Instant::now() + crate::provider::model::REFRESH_INTERVAL,\n"
     "            provider: ProviderId::Gateway,\n"
     "            entries: (1..=30)\n",
     "        shell.shell.apply(UiEvent::CatalogLoaded {\n"
     "            due: CatalogDue::at(catalog_now() + crate::provider::model::REFRESH_INTERVAL),\n"
     "            provider: ProviderId::Gateway,\n"
     "            entries: (1..=30)\n",
     1),
]
texts = {}
for path, old, new, count in FIXES:
    if reverse:
        old, new = new, old
    text = texts.setdefault(path, open(f"{root}/{path}", encoding="utf-8").read())
    found = text.count(old)
    if found != count:
        sys.exit(f"{path}: {found} matches where {count} were expected:\n{old}")
    texts[path] = text.replace(old, new)
    print(f"{path}: {count} replaced: {old.strip().splitlines()[1].strip()!r}")
for path, text in texts.items():
    open(f"{root}/{path}", "w", encoding="utf-8").write(text)
