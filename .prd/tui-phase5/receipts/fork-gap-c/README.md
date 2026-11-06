# P5-FK-GAPc receipt: every opener, Escape, undo, redo and select-all drop a waiting `/fork`

Branch `fix/p5-fork-status-gap`, on FK-GAPb (`b5868152`). This is the fix
round from the FK-GAPb reviews (zbrain
`workflow/2lab-ai/repos/xfx-tui-phase4/p5/FK-GAPb/reviews/`, `opus.md` and
`gpt6.md`).

## What was wrong

- **opus MUST: `C-p` opened `/model`'s catalog with the ask standing.**
  `model_catalog_shortcut` needs no line and no edit, and `open_models_with`
  cleared `fork_menu` but not `fork_asked`. The answer then closed the
  catalog and opened the fork menu (`fork_points`' own closes). The next
  key gave the catalog's draft back through `amended`, which closed the
  fork menu as well. So the FK-GAPb receipt's "nothing else in the slot can
  be up while a `/fork` waits" was false, and that receipt now says so at
  the claim.
- **opus SHOULD: an Escape in the window did not drop the ask.** The menu
  then opened after the user had pressed Escape, while the `/fork` parity
  row says Escape closes it.
- **Found while pinning:** upstream closes the fork and `/effort` menus on
  an undo, a redo and a select-all whether or not the draft changes. That
  is `dismissActiveMenusThenRedraw` first, at
  `src/core/app/app_input_runtime.zig:332-333`, `:379-380` and
  `:386-387@481104fd`. Here an undo went through `moved` and left both
  menus up, and a waiting ask too.

## The fix

- **Every opener of a slot surface drops the ask**: `self.fork_menu = None`
  became `self.close_fork_menu()` in `open_skills`, `help`, `resume`,
  `usage`, `permissions` (the rule confirmation), `statusline`,
  `workspace`, `mcp`, `settings` and `open_models_with`. That is the
  dispatcher's decision for `open_models_with`, applied to every opener,
  so the rule is one rule: whatever opens another surface while the
  `/fork` waits drops it.
  - Upstream blocks `C-p` over its open fork menu
    (`modelPickerShortcutBlocked`, `app_input_runtime.zig:1964-1967`). Here
    the menu is not open yet, and this unit already treats the window as
    the composer's (Up recalls), so `C-p` opens the catalog and the `/fork`
    is dropped.
- **Escape drops the ask** and does nothing else: a new first `Escape` arm
  in `act`. It is also counted as a key a menu takes, so it disarms other
  gestures.
- **`dismiss_for_edit`**: the close of the two menus that read no query
  (`/effort`'s, and `/fork`'s with its ask). `amended` uses it, and so do
  Undo, Redo and SelectAll before they act.
- **Docs:**
  - `fork_points` now says two rules keep one surface. The closes it keeps
    are **not decorative**: on the `C-p` path they were all that kept the
    fork menu from opening beside the catalog.
  - `close_fork_menu` lists every kind of caller.
  - `Disclosure::last_control_row` gives the rule and the gap as the
    reason, not the activity row.
  - The `/fork` row in `docs/parity.md` is extended.

## RED (`b5868152`'s production code, this round's tests)

On a throwaway clone of `b5868152`, with this round's test module spliced
in (`red-tests.txt`). **5 fail:**

| test | on `b5868152` |
|---|---|
| `a_fork_still_on_its_way_opens_nothing_over_the_catalog_ctrl_p_opened`: `fork_asked`, `C-p`, the answer | "the answer closed the catalog" |
| `an_escape_while_a_fork_is_on_its_way_drops_it` | "the menu opened after the Escape" |
| `every_command_that_opens_a_surface_drops_a_fork_on_its_way`: `/help` `/status` `/permissions` `/statusline` `/settings` `/usage` `/workspace` `/model` `/resume` `/mcp`, each handler reached through the router without the line-sent close | "/help: the answer opened the fork menu" |
| `the_skills_menu_opening_drops_a_fork_on_its_way` | the fork menu opened beside `/skills`' |
| `an_undo_a_redo_or_a_select_all_closes_the_fork_and_effort_menus_as_an_edit_does` | "undo left the fork menu up" |

These pass on `b5868152` too. They pin wiring that was already there
(test-only):

- `a_line_sent_while_a_fork_is_on_its_way_drops_it_whatever_put_it_there`:
  the `submit` close, reached on its own. The line is put in the composer
  directly, then sent as a prompt, so no surface it opens closes the ask.
- `effort_menu_opening_drops_a_fork_open_or_on_its_way`: the
  `effort_command` close, with the fork menu up and then with the ask
  waiting. The handler is called directly.
- `a_recalled_prompt_sent_while_a_fork_is_on_its_way_opens_no_menu_over_its_turn`
  (gpt6 1): recall, Return, the turn starts, the answer lands, no menu. The
  recall-only pin is kept as it was.
- `the_receipt_names_the_painted_row_of_the_last_choice_at_every_height_with_and_without_the_gap`
  (gpt6 2). Every height 7..=40 at 80 columns, under a blank tail (gap 0)
  and under a fork's plain line (gap 1). Wherever the question is inline,
  the receipt's row is the painted row of `3  Don't apply`, and a delivered
  frame is `ready`. At the fit boundary (`fits_approval` holds two rows for
  a turn's row), the gap is kept and does not move the boundary. One row
  shorter, the question goes to the alternate screen.
- `slot_occupants` (opus, gpt6 3) counts all 18 fields `slot()` reads.

## Each wiring reverted alone

`mutants.txt`: a throwaway clone of `9bd8f0ff`'s tree, one revert at a
time, the same test set each run.

| revert | fails |
|---|---|
| the `submit` close | `a_line_sent_while_a_fork…` ("the menu opened over the turn") |
| the `effort_command` close | `effort_menu_opening_drops_a_fork…` |
| `open_models_with` back to `fork_menu = None` | the `C-p` test, and the table at `/model` |
| the Escape arm | `an_escape_while_a_fork…` |
| `help` back to `fork_menu = None` | the table at `/help` |
| the Undo/Redo dismissal | `an_undo_a_redo_or_a_select_all…` ("undo left the fork menu up") |
| the rule confirmation's close (`permissions`) back to `fork_menu = None` | `a_rule_confirmation_opening_drops_a_fork_on_its_way` ("the fork menu opened under the confirmation") |
| `\|\| self.fork_asked` out of the Escape disarm | `an_escape_that_drops_a_waiting_fork_disarms_the_exit_as_a_menus_escape_does` ("the Escape left the exit armed") |

The last two rows, and gap 0 asserted for every height without a fork in
`the_receipt_names_the_painted_row_…` (it rested on `transcript.rs:699`,
the welcome line ending blank), are the review FK-GAPc pins from opus,
added test-only in the final step.

## Residual (named, not fixed)

`/fork` is not refused during a manual `/compact`: its refusal reads
`provider_work_busy` (`activity.working()` or a prompt waiting), and a
compaction is `compacting()`. So in that window the first Escape drops the
ask before it would arm the compaction's interrupt. fx refuses `/fork` only
on `app.stream.active` (`app_session_runtime.zig:2078@481104fd`), and
whether a manual compaction sets `stream.active` has not been read. Left
for a unit that reads it.

## Final step: on `main` after FK-RULES (#55)

**The merge.** `8aead681`, "Merge remote-tracking branch 'origin/main'
into fix/p5-fork-status-gap": `git merge --no-edit origin/main` at
`a592c3a2` (PR #55) onto `f498cb02`. One conflict, in `docs/parity.md`, on
the `/fork` row. Each side had changed a different span of that one line,
against the merge base `cf25feaf`:

- this branch: the sentence on edits closing the menu (now: recall,
  Escape, line sent, undo/redo/select-all, any surface opening) and the
  sentence on the plain line (now: the band's blank row above a panel);
- FK-RULES: the sentence on the fork's permissions (now: the
  saved-session rules, each remembered deny and no allow, and its
  **Divergence**).

The resolution is both sides' edits applied to the base line. It was
checked word by word: the result differs from this branch's line only by
FK-RULES' span, and from FK-RULES' line only by this branch's two spans.
`git show --remerge-diff 8aead681 -- docs/parity.md` shows just that line.
The merge brought no change to `src/tui/shell.rs`, so no slot opener was
added by it.

**Pairs.** `sweep.yml`'s job, run locally: `ci-sweep.sh --base a592c3a2
--fx <fx v0.0.16> --pin v0.0.16 --xfx <8aead681 debug> --base-xfx
<a592c3a2 debug> --jobs 2`, `FX_AUTO_UPGRADE=0`. 42 scenarios, **313 snap
pairs: 205 pass at the head, 0 new, 0 regressions**; 1,396 s. The only
retried scenario was `p5-permissions-early-probe-ask-after`, which is
approved either way. Every `p5-effort-fork-*` pair is at its count above:
fork 15/1/16, fork-edge 10/1/11, fork-empty 4/0/4, fork-continue 2/0/2,
effort-menu 18/0/18, effort-column 17/0/17, effort-{backtab, ctrlc, down,
loading} 2/0/2 each, effort-nolevels 5/0/5, effort-short 3/0/3, slash-rank
3/0/4, oldread-src 0/0/3, oldread-v1 2/0/4, oldread-v2 2/0/7. The
`p5-panels-*` and `p5-permissions-*` pairs keep the FK-GAP receipt's
counts.

**Gates** at `8aead681`, `TMPDIR=/private/tmp/claude-501/fsg`:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,753 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection` | 4,782 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |

No pty test failed, so none was re-run.
