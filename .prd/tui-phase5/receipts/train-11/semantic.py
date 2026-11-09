#!/usr/bin/env python3
"""Train 11, merge 2 (CATALOG-REFRESH): the two semantic conflicts git's
line merge cannot see -- each side's new tests written against a type the
other side changed -- fixed in the merge commit, so its tree builds. Each
fix is spelt as the tree already spells it:

1. train 10's join test on main
   (`a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id`,
   src/tui/shell.rs) builds `CatalogLoaded` and `CatalogRefreshFailed`
   with an `Instant` due; CATALOG-REFRESH made every such due a
   `CatalogDue` on the catalog's clock. Rewritten as CATALOG-REFRESH
   rewrote the same two expressions in its own tests: a load's due
   `CatalogDue::at(catalog_now() + REFRESH_INTERVAL)`, a refresh failed
   just now `CatalogDue { since: Some(now), at: now + failure_backoff(1) }`
   (its `refresh_failed`).
2. CATALOG-REFRESH's two worker tests that build a `Queue` by hand
   (`a_told_ask_put_aside_behind_a_switch_starts_the_first_load_once_and_never_a_refresh`,
   `switched_to_gateway`) lack the field COMPACT-NOTICE-LATE (train 10)
   added: `owed: 0`, as `turn_loop` and every other literal on main start it.

    python3 -I -B semantic.py <worktree>
    python3 -I -B semantic.py --reverse <worktree>   # takes them out again

Each replacement must match exactly the number of times given, or nothing
is written. `--reverse` is for the check from both ends (merge-checks.sh):
the merge commit's two files with the four taken out are git's merge,
ours -> it exactly theirs' hunks and theirs -> it exactly ours'.
"""
import sys

reverse = sys.argv[1] == "--reverse"
root = sys.argv[-1]
FIXES = [
    ("src/tui/shell.rs",
     "            due: Instant::now() + REFRESH_INTERVAL,\n            provider: ProviderId::Gateway,\n            entries,\n",
     "            due: CatalogDue::at(catalog_now() + REFRESH_INTERVAL),\n            provider: ProviderId::Gateway,\n            entries,\n",
     1),
    ("src/tui/shell.rs",
     "                retry,\n                due: Instant::now() + crate::provider::model::failure_backoff(1),\n            });\n",
     "                retry,\n                due: {\n                    let now = catalog_now();\n                    CatalogDue {\n                        since: Some(now),\n                        at: now + crate::provider::model::failure_backoff(1),\n                    }\n                },\n            });\n",
     1),
    ("src/tui/worker.rs",
     "                outstanding: &outstanding,\n                taken: 0,\n            };\n",
     "                outstanding: &outstanding,\n                taken: 0,\n                owed: 0,\n            };\n",
     1),
    ("src/tui/worker.rs",
     "            outstanding: &outstanding,\n            taken: 0,\n        };\n"
     "        let cancel = Cancellation::new(CancelToken::new());\n        let ended = on_a_runtime(run_setup(\n",
     "            outstanding: &outstanding,\n            taken: 0,\n            owed: 0,\n        };\n"
     "        let cancel = Cancellation::new(CancelToken::new());\n        let ended = on_a_runtime(run_setup(\n",
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
    print(f"{path}: {count} replaced: {old.strip().splitlines()[0]!r}")
for path, text in texts.items():
    open(f"{root}/{path}", "w", encoding="utf-8").write(text)
