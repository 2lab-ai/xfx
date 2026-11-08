# F2 approvals — fx `c1db919` vs xfx, 40x120

Two scenarios, each driven identically on both binaries by `scripts/fxcap/capture.sh`
(isolated tmux socket, empty `HOME`, fake key, loopback fake gateway, desktop shims):

```
export TMPDIR=<tmp>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  approvals     40x120 <out>
XFX_BIN=target/release/xfx scripts/fxcap/capture.sh xfx approvals     40x120 <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  approvals-big 40x120 <out>
XFX_BIN=target/release/xfx scripts/fxcap/capture.sh xfx approvals-big 40x120 <out>
```

`approvals.cap` reads `a.txt`, then asks about an edit (walked with Down, wrapped, amended with Tab
+ `use capitals`, applied with Enter), a new file `c.txt` (refused with **Ctrl-C**), a command
(allowed for the session with **`2`**), and a second command (**Escape**). `approvals-big.cap`
asks about an 80-line new file. Each binary gets its own turns file (`approvals-fx.json` /
`approvals-xfx.json`: fx's `shell` is xfx's `terminal`) and its own `ask` switch, through the
harness's new `fx: ` / `xfx: ` line prefix. Scratch paths are replaced with `<scratch>`; the
request bodies the gateway received are not committed (size) and are quoted below.

## What matches

Row for row, and byte for byte in the `.ansi` captures (tmux's re-serialization of both):

| snapshot | fx rows | xfx rows | identical |
|---|---|---|---|
| `edit-ask` | 8-20, 21-23 | 10-22, 25-27 | every row but choice 2's wording: the solid rule, the four review rows (`      1   alpha line 1` / `      2 - alpha line 2` / `      2 + ALPHA LINE TWO` / `      3   alpha line 3`), the dotted rule, `Permission needed · Review change … Edit · +1  -1`, `a.txt  ·  Apply this change?`, `❯ 1  Apply once`, `3  Don't apply`, the rule, the hint |
| `edit-down1` | 18, 23 | 20, 27 | the moved marker and the hint changing to `↑↓ or tab options` on choice 2 |
| `edit-down3-wrap` | — | — | Down past the third choice wraps to the first on both |
| `edit-amend-typed` | 18 | 20 | `❯ 1  Apply once, use capitals` (the text; see below for the cursor cell) |
| `write-ask` | 12-13, 15, 17 | 14-15, 17, 19 | `1 + new file`, `2 + second line`, `Write · +2  -0`, `c.txt  ·  Apply this change?` |
| `after-cc-cmd-ask` | 13-17, 19-26 | 16-20, 22-26, 29-31 | `Permission needed · Choose one … Command`, the bold question, the bold `    echo hi > out.txt && cat out.txt`, `  ❯ 1. Yes` (marker outside the bold), `2. Yes, and don't ask again for this exact command`, `3. No`, and the hint at column 0 |
| `approvals-big write-big` | 30-40 | 28-40 | the review's tail against the dotted rule, `Write · +80  -0`, `big.txt  ·  Apply this change?`, the choices, the rule |

SGR, as captured on both: header `ESC[1m…ESC[0m` then `ESC[38;5;245mEditESC[39m · ESC[38;5;252m+1ESC[39m  ESC[38;5;252m-1ESC[39m`
two cells from the right edge; review numbers `ESC[38;5;245m`, context sign and text `ESC[38;5;245m`, `+`/`-`
and changed text `ESC[38;5;252m`; dotted rule `ESC[38;5;240m┄…`; path `ESC[1ma.txtESC[0m`; marked file choice
`  ESC[1mESC[38;5;255m❯ 1  Apply onceESC[0m`, unmarked `    ESC[38;5;245m2ESC[39m  …`; marked command choice
`  ❯ ESC[1mESC[38;5;255m1. YesESC[0m`; command rows `ESC[1mESC[38;5;255m  $ …ESC[0m`; hint `ESC[38;5;245m…` (file:
indented two, command: column 0).

Behaviour, from the gateway logs and request bodies:

- **Ctrl-C refuses only the call, and the turn goes on.** fx: `write_file` → `execution-denied`
  (`user_denied`), then the next request asks for `shell`. xfx: `write_file` → `write_file was not
  permitted: you declined \`write_file\` for \`<scratch>/work/c.txt\``, then the next request asks for
  `terminal`. Both screens then show the command question.
- **`2` answers at once** and allows that exact command: both run `echo hi > out.txt && cat out.txt`
  (`work-end.txt`: `out.txt` holds `hi`) and ask again for `echo again`.
- **Escape cancels the turn.** Both gateway logs end at the request that asked for `echo again`: no
  further model request was made (fx 6 requests incl. the title, xfx 5).
- **The amendment travels after the tool result**: fx's next request ends
  `tool-result edited a.txt (41 bytes)` then `{"role":"user","text":"use capitals"}`; xfx's ends
  `tool-result Edited a.txt` then `{"role":"user","text":"use capitals"}`.

## What differs, and why

| # | fx | xfx | why |
|---|---|---|---|
| 1 | `2  Apply + allow workspace file access for this session` | `2  Apply + allow future edits to this file for this session` (`writes` for a write) | xfx's `Always` grants this tool on this file (`crate::permission::Grant`), not workspace file access; the label says the grant xfx really makes |
| 2 | (none) | a dim `2 = allow every future edit_file to \`a.txt\` … and in every later \`xfx ask --resume-id …\` of this saved session` under the choices | xfx's grant outlives the process into a resumed session, and the readiness gate requires that scope disclosed whole before `2` is accepted (`approval_readiness.rs`, `scope_whole`) |
| 3 | `$ # shell.run profile=user shell=/bin/zsh` | `$ # terminal.exec cwd=.` | xfx's tool is `terminal`/`exec`, and the cwd is part of what a grant is keyed by; xfx runs an approved command under `/bin/sh` or without a shell at all, so a `shell=` would be wrong for half of them |
| 4 | `and tell fx what to do next` with a reverse-video first cell (`ESC[0;7m`) | `and tell xfx what to do next`, no reverse video | the product's own name; xfx places the terminal's real caret in the draft instead of painting a fake one, and reverse video is outside the painter's vocabulary (`pacer::is_palette_sgr`) |
| 5 | activity row hidden while the question is up | `• Running (0s) (↑4 ↓0)` and a blank above the question's rule | upstream's `render_input.zig:705`; unit F1's row (`.prd/tui-phase4/loop.md`), not this unit |
| 6 | `● N tool calls · …` tree above the question; `└ Cancelled echo again` + `■ Cancelled echo again · What can fx do differently?` after Escape | `[tool] … running/ok/refused` rows; `xfx: interrupted -- …` and `the turn was cancelled` after Escape | the transcript's tool rows are F1's and the cancel wording is F4's |
| 7 | amendment shown as a prompt card `┃ use capitals` | `[you] use capitals` | the transcript row is the notices unit's |
| 8 | the big review sits under the transcript on fx's screen | blank rows above a short review on the plane | fx's screen is one document (transcript + review + chrome); xfx's plane cannot read back the terminal's scrollback |
| 9 | `wheel scroll` in the hint; the wheel walks the review | `ctrl+p/n scroll`; `C-p`/`C-n` walk it | xfx asks for no mouse reports. (tmux delivered no wheel to fx under capture, so fx's `write-big-walked` is unchanged; xfx's moved three rows) |
| 10 | edit asked without a prior read | the scenario reads `a.txt` first, on both | xfx's `edit_file` refuses a file the session has not read (`src/tools/mutate.rs`), a tool-layer rule (ssot Tensions) |
| 11 | banner `𝒇x v0.0.13` | `x𝒇x v0.1.0` | the welcome row is another unit's |
