# E1 receipt: the `@` file picker and the composer keys, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, a fake key and a loopback fake gateway:

```
scripts/fxcap/capture.sh {fx,xfx} filepicker        40x120 <dir>
scripts/fxcap/capture.sh {fx,xfx} filepicker        24x80  <dir>
scripts/fxcap/capture.sh {fx,xfx} filepicker-keys   40x120 <dir>
scripts/fxcap/capture.sh {fx,xfx} filepicker-narrow 24x80  <dir>
scripts/fxcap/capture.sh {fx,xfx} filepicker-narrow 40x120 <dir>
```

The three scenarios are added in this unit. `filepicker` and `filepicker-keys` work in `scenarios/filepicker-ws/` (the research captures' `ws2`: `Cargo.toml`, `README.md`, `docs/guide.md`, `src/{main,lib}.rs`, `src/deep/sched.rs`); `filepicker-narrow` works in `scenarios/filepicker-long-ws/` (three 60-character names under a 30-character directory, the shape of fx's e2e `tui-file-picker.test.ts:1202`). Neither workspace is a git work tree, so both binaries index it by walking it.

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` built from this branch's working tree just before the commit, so its banner names the parent commit (`v0.1.0-b9c5591`). Each snap has a `.txt` file (text) and an `.ansi` file (`capture-pane -e`). "Band rows" are the rows from the composer's first row (`┃ …`) down to the hint row; "identical" counts the rows that are byte-equal in the `.ansi` files at the same offset from that row.

**Sanitized** means: every file was scanned for the run directory (`fxcap.*`), home and temp paths, and the fake key. The run directory appears only in fx's two `002-completion.json` request bodies (fx's system prompt names the workspace and home), and was replaced by `<scratch>` there; nothing else matched.

## Verdict

**Every band row of every snap is byte-identical, at both sizes: 33 snaps, and in each of them the whole screen differs only in row 1, the brand line** (`𝒇x v0.0.13` against `x𝒇x v0.1.0-b9c5591 [debug]`, xfx's own brand, WU-A). That includes the SGR: the marked row is fx's `approval_button_inactive_style` (tmux shows `ESC[38;5;255mESC[48;5;239m … ESC[39mESC[49m`; both binaries write the background first), the other rows `ESC[38;5;245m`, a matched run `ESC[1m … ESC[0m` with the row's style put back after it, the rules `ESC[38;5;240m`, and the hint row unchanged (`ESC[38;5;252mauto ESC[38;5;245m· model-a`).

| Scenario | Size | Snaps | Band rows identical |
|---|---|---|---|
| `filepicker` | 40x120 | `at`, `at-sch`, `at-esc`, `at-dir`, `at-dir-down`, `at-dir-cj`, `at-tab`, `at-enter`, `submitted`, `at-claim` | 10/10, 10/10, 3/3, 10/10, 10/10, 10/10, 3/3, 3/3, 3/3, 10/10 |
| `filepicker` | 24x80 | the same ten | the same counts |
| `filepicker-keys` | 40x120 | `alt-d`, `alt-b`, `alt-f`, `pgup`, `pgdn`, `at-pgdn`, `at-ck` | 3/3, 3/3, 3/3, 5/5, 5/5, 10/10, 10/10 |
| `filepicker-narrow` | 24x80 | `at-alpha`, `at-alpha-down`, `at-deep` | 10/10 each |
| `filepicker-narrow` | 40x120 | the same three | 10/10 each |

**Re-run after merging `plan/tui-phase4` (835f389) into this branch** (merge `26fd238`, `target/debug/xfx` rebuilt from it): the same 33 snaps, again identical beyond row 1. The first 40x120 `filepicker` attempt at load average ~95 typed its `@` before the xfx session had taken the terminal, so the keystrokes were lost and three snaps came out empty; it was retaken. The files in this directory are the pre-merge captures.

## What each snap shows (the same on both sides)

- `at` -- `@` in an empty composer: six rows two columns in (`Cargo.toml` marked, `README.md`, `docs/`, `docs/guide.md`, `src/`, `src/deep/`) between two rules, the index's own byte order, directories with their `/`. The statusline stays on the hint row: fx's hint switch has no file-picker arm (`paint_plan.zig:946-978`), so there is no key help to show.
- `at-sch` -- `@sch`: one row, `src/deep/` then bold `sch` then `ed.rs` on the bar; five blank rows keep the list's height (`activeListPickerReservedRows`, `picker_presentation.zig:634-639`).
- `at-esc` -- Escape closes the list and keeps `@sch`; no `esc again to clear`.
- `at-dir` / `at-dir-down` / `at-dir-cj` -- `see @src/` browses the directory (`queryMode`, `path_completion.zig:25-27`): `src/deep/`, `src/lib.rs`, `src/main.rs`, six columns in under the `@` (`projectedAnchorColumn`, `visual_layout.zig:704-712`). Down moves the bar to `src/lib.rs`; C-j moves it on to `src/main.rs` (`routePickerControlByte`, `app_input_runtime.zig:1220-1235`).
- `at-tab` -- Tab on `src/main.rs`: `see @src/main.rs ` and the list closes.
- `at-enter` -- `@gui` and Enter: `@docs/guide.md ` and nothing sent; `submitted` -- the second Enter sends it.
- `at-claim` -- after another `@gui` + Enter, typing ` @REA` leaves one space between the mentions (the completion's separator is claimed, e2e `tui-file-picker.test.ts:2005`), and the new `@REA` lists `README.md` under its own `@`, column 18.
- `alt-d` -- C-a then Alt-d on `hello, world`: `world` (the word and the `, ` after it, `wordRightRange`, `composer_deletion.zig:138-164`).
- `alt-b` / `alt-f` -- Alt-b twice then `Y`, Alt-f then `Z`: `world YfooZ bar` (word moves by word characters, to the end of a word, `horizontal_navigation.zig:85-141`).
- `pgup` / `pgdn` -- `line1\` + Enter continues the line (`app_input_runtime.zig:1612`), then C-j, then PgUp moves the caret to the first row (`<` lands after `line1`) and PgDn back to the last (`>` after `line3 `): a page is the composer's row cap and never a history step.
- `at-pgdn` / `at-ck` -- with the list up, PgDn moves the bar a page, which on a nine-row list wraps to the first row (`advanceSelection`, `list_window.zig:40-48`); C-k moves it up, wrapping to `src/main.rs`, scrolling the window.
- `at-alpha` / `at-alpha-down` (narrow) -- a path wider than the row gives up its directory first: `deeply…ctory/` (twelve cells, cut in the middle) then the whole name at 80 columns. `at-deep` -- the same paths under an `@` in column 27: the name is cut too, `alpha-component-01-with-a-very…e-name.zig` (`appendFilePickerLabel`, `picker_presentation.zig:920-957`).

## What does not match, and why

- **The request text.** fx sends `"text":"@docs/guide.md "` (`fx-filepicker-40x120-bodies/002-completion.json`), xfx `"text":"@docs/guide.md"` (`xfx-…-bodies/001-completion.json`). The mention itself is the same plain text in both, no attachment; the trailing space is trimmed because xfx trims every prompt it sends (`src/interactive.rs:436-441`, `classify`) -- not this unit's surface, and not visible on the screen.
- **fx's title request.** fx asks its gateway for a session title first (`001-title.json`); xfx does not. Unrelated to the picker.
- **Not captured, unit-tested instead:** the `indexing files...` row (both binaries index nine files faster than a capture can see), `no matching files` (`shell.rs` `an_unmatched_mention_is_sent_by_enter`), `Directory unavailable. tab to retry; esc to dismiss.` and the git work-tree listing (`file_index.rs` `a_work_tree_is_listed_by_git_and_its_ignore_rules_hold`).
- **Keys this unit leaves alone:** Shift-Tab (the `/permissions` unit binds it to the runtime mode switch), Ctrl-P (stays history-previous until D2's model catalog exists, ssot divergence 8), Ctrl-V (fx attaches a clipboard *image*, `app_input_runtime.zig:1562-1568` → `image_commands.zig:86-103`; xfx's request has no image attachment, so it is class B and stays unbound).
