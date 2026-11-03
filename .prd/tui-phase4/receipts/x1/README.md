# X1 receipt: a paste with input behind its end marker in one read

One scenario, `scripts/fxcap/scenarios/paste-suffix.cap` -- the Phase 5
research's `p5-input-paste.cap`, its keystrokes copied byte for byte -- run at
40x120 on both binaries with the repo's driver, unmodified:

```
FX_BIN=<fx c1db919 build, 0.0.13> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  paste-suffix 40x120 .prd/tui-phase4/receipts/x1
                                  TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx paste-suffix 40x120 .prd/tui-phase4/receipts/x1
```

It types `prior`, then **one** `type` of `ESC[200~new ESC[201~ CR`: one write,
so one read, with an Enter behind the paste's end marker.

The fx binary is the restored c1db919 build (sha256 `7b76319a…`, `--version`
0.0.13), run with `FX_AUTO_UPGRADE=0` as every fx run of this driver is. The
xfx binary is `target/debug/xfx` built at `bb78139` -- the unit with
`plan/tui-phase4` `49f32a9` merged, which is what its banner says -- and the
xfx files here were retaken with it on 2026-11-03. Against the take before it
(at the fix commit `ad5bf82`, before the merge) the screens are the same
bytes but for the banner's hash; what moved is the gateway log's port. No
scenario fixtures: the fake gateway is given no turns, so a submitted prompt
would show up as a request body.

## Result

**One snap pair (`paste-enter`, screen and `.full`); the only row that differs
is row 1, each product's banner.** Rows 2 on are byte for byte the same in the
`.txt` and the `.ansi` files, and `../m1/cells.py` reports `0 rows differ in
their cells (row 1 not compared)` for both pairs.

| snap | fx c1db919 | xfx | verdict |
|---|---|---|---|
| `paste-enter` | `✗ input: Paste was not applied because extra input followed its end marker.`, composer `┃ prior` | the same, same SGR | identical (row 1 only) |
| `paste-enter.full` | the same, whole history | the same | identical (row 1 only) |
| `gwlog` | `GET /coding-agent/v1/models` only, `bodies/` empty | the same | nothing submitted on either side |

What the screens say, in fx's words: the paste was refused whole (the
composer is `prior`, not `priornew`), the Enter behind the end marker was not a
key (nothing was sent: the gateway saw the catalog read and no completion
request), and the refusal is an error notice under `input`
(`input_paste_runtime.zig:179-193`). Before this unit xfx applied the paste at
its marker and submitted `priornew` on the same Enter -- the e2e that pins the
rule, `a_paste_whose_end_marker_shares_a_read_with_enter_is_refused_and_submits_nothing`
(`tests/tui.rs`), failed on `84805786` with `priornew` on the screen.

## The rule, in fx c1db919's source

- **Same delivery, not same read.** A paste is settled only when the event
  loop's readiness check finds the input drained after a burst of reads
  (`src/ui/event_loop.zig:44-72`, `:118`); a burst that hit its 32-read cap with
  input still waiting carries on unsettled (`:60-63`, `:109-112`). xfx's
  `read_burst` ends a delivery at the same check, and a read takes 128 bytes,
  so an Enter one read behind the marker in the same write is still behind it.
- **What is behind the marker.** Once `ESC[201~` matches, the boundary is
  `end_candidate`; any further byte makes it `unsafe` and is only counted
  (`src/core/input/paste_framing.zig:93-108`, `:141-157`) -- a second end marker
  and a new paste's start included (test `:548-565`;
  `src/core/app/app_input_runtime.zig:9841-9860`). Settled, `end_candidate`
  finishes and `unsafe` rejects (`paste_framing.zig:160-167`).
- **Not applied.** A rejection resets the capture and writes the error notice;
  nothing is inserted (`input_paste_runtime.zig:179-193`). The bytes behind
  the marker went to the paste's counter and were never decoded as keys.
- **Every owner** (`input_paste_runtime.zig:105-115`): the composer
  (`app_input_runtime.zig:9862-9873`), an approval amendment and an approval
  with no draft open (`:10131-10158`), a question's freeform answer
  (`:10160-10182`), and a question with a choice marked (`decision_prompt`).
  xfx covers each in `src/tui/shell.rs`'s tests.
- **One exception, kept**: a capture that had already overflowed its budget
  settles as finished whatever followed it (`paste_framing.zig:165`, test
  `:448-464`), and finishing it is the input-limit refusal -- so that sentence
  is said instead. Nothing is applied either way.

fx v0.0.16 changed this (9e0eab41: the paste is kept and only the input behind
it is dropped, with a `!` warning). That is Phase 5's, not this receipt's.

**Sanitized:** nothing -- these snaps name no run directory. The gateway logs'
ports are each run's own. `.ansi` files are tmux's re-serialization of cell
attributes (`capture-pane -e`), not the programs' wire bytes (see
`../c0/README.md`).
