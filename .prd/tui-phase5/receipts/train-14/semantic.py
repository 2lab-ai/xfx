#!/usr/bin/env python3
"""Train 14 (new): the merges' semantic fixes -- text git merged clean that
does not build -- each written down here, so `reproduce.sh` can redo it.
A fix is a text replaced in one file, with the number of places it must be
found; any other count stops the script, changing nothing.

Merge 2 (CTRLO-OTHER-PICKERS onto merge 1): three of the unit's new tests in
`src/tui/shell.rs` (`ctrl_o_over_the_model_menu_holds_no_frame_and_its_keys_stay_its`,
`c_j_in_ctrl_os_burst_over_model_session_and_skills_menus_does_nothing_and_c_k_kills_nothing`,
`c_k_in_ctrl_os_burst_with_text_after_the_caret_kills_it_and_each_menus_search_follows`)
apply a `UiEvent::CatalogLoaded` whose `due` is an `Instant`, as at the
unit's base; main's CATALOG-REFRESH (train 11, `2810f53c`) made `due` a
`CatalogDue` on the catalog's clock and wrote every such test line as below
(main's `shell.rs` holds 35 of them). The three take main's form, nothing
else.

Merge 3 (PAGE-KEYS-SETTINGS-COLUMN onto merge 2): the unit's check in
`Shell::consume_inputs` (`2cbf4d41`) hands `C-j`/`C-k` under the full
transcript to the `@` list first, leaving the list's keys there to unit
CTRLO-AT-KEYS (its comment). At the unit's base the list took them; at
CTRLO-AT-KEYS' head (`cb7c9c18`) `offer_to_files` refused them under the
screen, and CTRLO-OTHER-PICKERS (merge 2) moved that refusal into
`consume_inputs` (`composer_control`, after `offer_to_mcp`), deleting it
from `offer_to_files`. Merged as git merged it, the unit's check is first
and `offer_to_files` takes them: the `@` list moves under Ctrl-O again
(CTRLO-AT-KEYS' two cases fail, `red3.txt`). The fix does what the unit's
comment defers to: the call to `offer_to_files` goes, and both keys go to
the composer, the list's included. Nothing else in the check changes.
Then two that do not build: the unit's test helper `walked<T>` (`01d661ba`,
"the rows a list's mark walks under Down") has the name of AT-FOLDERS'
`walked` (`d9b9cdd8`, its round 6: the index's walks done) in the same test module, and is
`rows_walked` here, its five calls with it; and three of the unit's tests
build `due` as an `Instant`, which take main's `CatalogDue`, as in merge 2.

Merge 4 (IMAGE-LOSS-COPY-FX onto merge 3): the unit deletes
`ReplayedHistory::notices` and `Conversation::unsaid` -- the history holds
no notice any more -- and deletes its own tests' `notices.is_empty()`
asserts with them. Train 13's join test in `src/tui/worker.rs`
(`a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning`,
on main) still asserts `history.notices.is_empty()`: that assert goes, as
the unit's own went, and the test's comment says why nothing can say it.

Each fix is a `build` fix (the tree does not compile without it) or a
`behaviour` fix; `--only build` applies the build fixes alone, which is the
tree `red3.sh` tests to show the behaviour fix is needed.

    python3 -I -B semantic.py [--only build] <merge-number> <worktree>
"""
import sys

FIXES = {
    "2": [
        (
            "build",
            "src/tui/shell.rs",
            "due: std::time::Instant::now() + crate::provider::model::REFRESH_INTERVAL,",
            "due: CatalogDue::at(catalog_now() + crate::provider::model::REFRESH_INTERVAL),",
            3,
        ),
    ],
    "3": [
        (
            "behaviour",
            "src/tui/shell.rs",
            """            // they go on to the composer, a newline and a kill to the end. The
            // `@` list still sees them: its keys there are unit
            // CTRLO-AT-KEYS's (`docs/parity.md`).
            if self.full.is_open()
                && matches!(event, Input::Action(Action::ControlJ | Action::ControlK))
            {
                if !self.offer_to_files(&event) {
                    if let Input::Action(action) = event {
                        self.act(action, now);
                    }
                }
                continue;
            }""",
            """            // they go on to the composer, a newline and a kill to the end --
            // the `@` list's too, whose keys there are unit CTRLO-AT-KEYS's
            // (`docs/parity.md`).
            if self.full.is_open()
                && matches!(event, Input::Action(Action::ControlJ | Action::ControlK))
            {
                if let Input::Action(action) = event {
                    self.act(action, now);
                }
                continue;
            }""",
            1,
        ),
        (
            "build",
            "src/tui/shell.rs",
            "    fn walked<T: PartialEq + Clone + std::fmt::Debug>(",
            "    fn rows_walked<T: PartialEq + Clone + std::fmt::Debug>(",
            1,
        ),
        ("build", "src/tui/shell.rs", "walked(&mut shell, &", "rows_walked(&mut shell, &", 5),
        (
            "build",
            "src/tui/shell.rs",
            "due: std::time::Instant::now() + crate::provider::model::REFRESH_INTERVAL,",
            "due: CatalogDue::at(catalog_now() + crate::provider::model::REFRESH_INTERVAL),",
            3,
        ),
    ],
    "4": [
        (
            "build",
            "src/tui/worker.rs",
            """        // `types.zig:1668-1682@481104fd`) -- `ReplayedHistory::notices`, the
        // only source of the TUI's `! session:` lines
        // (`Conversation::unsaid`), says nothing of it -- while the call and
        // its answer document still go; back on the authority that sealed it,
        // the state goes too.
""",
            """        // `types.zig:1668-1682@481104fd`) -- since IMAGE-LOSS-COPY-FX the
        // history holds no notice at all, so nothing can say it -- while the
        // call and its answer document still go; back on the authority that
        // sealed it, the state goes too.
""",
            1,
        ),
        (
            "build",
            "src/tui/worker.rs",
            """            assert!(
                history.notices.is_empty(),
                "{wire:?} {model}: {:?}",
                history.notices
            );
""",
            "",
            1,
        ),
    ],
}

args = sys.argv[1:]
only = None
if args[0] == "--only":
    only, args = args[1], args[2:]
merge, root = args
fixes = [f for f in FIXES.get(merge, []) if only is None or f[0] == only]
for kind, path, old, new, count in fixes:
    text = open(f"{root}/{path}", encoding="utf-8").read()
    found = text.count(old)
    if found != count:
        sys.exit(f"merge {merge}: {path}: {found} places, not {count} -- read by hand")
    open(f"{root}/{path}", "w", encoding="utf-8").write(text.replace(old, new))
    print(f"merge {merge}: {kind} fix: {path}: {count} place(s), {len(old.splitlines())} lines -> {len(new.splitlines())}: {(old.splitlines() or [""])[0].strip()!r} ...")
print(f"# merge {merge}: {len(fixes)} fixes" + (f" ({only} only)" if only else ""))
