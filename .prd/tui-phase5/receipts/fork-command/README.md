# P5-FK2 receipt: the `/fork` TUI

The unit is `/fork` from fx v0.0.16 (`vercel-labs/fx@481104fd`, commit
`e4348723`; D58) on top of P5-FK1's store primitive
(`SessionStore::fork`, branch `feat/p5-store-fork` at `2fbc088e`, FK1b
included). The spec is `research/tui-phase5:.prd/tui-phase5/research/effort-fork.md`
§6 unit 4, with its §2-§4 screens, rules and durable shape: the Fork menu,
the three refusals, the switch, the fork's two lines, the composer refill,
the window title, the zero-turn listing and the line shell's sentence.

## How it was captured

Four scenarios at 40x120:

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  p5-effort-fork-<s> 40x120 <dir>
XFX_BIN=<this branch's target/debug/xfx>  scripts/fxcap/capture.sh xfx p5-effort-fork-<s> 40x120 <dir>
```

- Both binaries ran with the same `TMPDIR`, outside the repository.
  `capture.sh` launches fx with `FX_AUTO_UPGRADE=0`, and after every run
  found the reference binary's bytes unchanged (sha256 `8e6cda2e…`).
- `fork-empty` is the research's scenario unchanged.
- `fork` and `fork-edge` are **FK2's copies**. Each drops the research
  run's `/status` step (`snap status`, `snap status-empty`): the `/status`
  panel and its `· saved` are P-status's (D90), and xfx's `/status` on this
  base writes into the document, which would move every later snap.
  Nothing else changed.
- `fork-continue` is **new**. It forks before the only prompt, then sends
  the refilled prompt into the zero-turn fork. It pins the blank row between
  the fork's plain line and the next card, and the fork's first request.

Every file here went through `scripts/fxcap/redact.py`. `.ansi` files are
tmux's re-serialization of cell attributes (`capture-pane -e`), not the
programs' wire bytes.

## RED (the base, `2fbc088e`)

`red/` holds the base binary's screens, and `red.txt` is `cells.py` over
every pair. **All 31 differ.**

- Bare `/fork` answers `✗ command: Unknown command. Try /help.`
  (`red/xfx-…-fork-edge-40x120-at-launch.txt`, row 3; `-fork-40x120-menu`,
  row 25).
- So nothing forks: every snap after the first `/fork` shows the source
  session, with that refusal in it.

## GREEN

`green.txt` is `cells.py` (`.prd/tui-phase4/receipts/m1/cells.py`) over
every `fx/` and `xfx/` pair, row 1 (each product's banner) left out.

**17 pairs are identical as captured:**

| scenario | snaps |
|---|---|
| `fork` | `menu`, `up1`, `up2`, `up3-clamp`, `down`, `esc`, `resume-both`, `resume-both-down`, `resumed-fork` |
| `fork-edge` | `at-launch`, `with-arg`, `midturn`, `menu`, `edit-closes` |
| `fork-empty` | `menu-one`, `resume`, `resume-all` |

**13 pairs are identical but the product's name and the session id**, which
`cells.py` compares raw. These rows:

- `original session fx --resume <12 chars>` against
  `original session xfx --resume <13 digits>-<16 hex>`;
- `Continue session with: …` on the exit line;
- tmux's own `Pane is dead (status 0, <clock>)`.

They are D1's precedent ("identical but the product name and the id",
`.prd/tui-phase4/receipts/d1/README.md:140`). The parity ledger's
`product-name`, `session-id` and `tmux-dead-pane` normalizations
(`scripts/fxcap/parity-ledger.toml`) leave each of the 13 with no differing
row:

| scenario | snaps |
|---|---|
| `fork` | `forked`, `forked.full`, `continued`, `resume`, `quit` |
| `fork-edge` | `forked-first`, `forked-first.full`, `fork-of-empty`, `resume`, `quit` |
| `fork-empty` | `forked` |
| `fork-continue` | `forked`, `continued` |

**1 pair differs: `fork/slash-fo`.** fx lists `/fork` and `/effort`
(`Commands 2`), and the name column is padded to `/effort`. This base has
no `/effort`: that is P5-EF1's. The row is `Commands 1` with `/fork` alone.
The pair matches once both units are on one branch.

The xfx binary is `target/debug/xfx` built from this branch's tree before
its commit, so its banner names the base revision `2fbc088`.

### What the pairs show

- **The menu** (`fork/menu` … `esc`, `fork-edge/menu`, `fork-empty/menu-one`):
  - no title;
  - the long prompt cut at its third row, ending `an…`;
  - the newest prompt selected (bold 255), the rest dim 245;
  - Up ×3 stops on the oldest;
  - Escape restores the footer;
  - `x` closes the menu and stays in the composer (`edit-closes`).
- **The refusals**:
  - `* session: nothing to fork yet` at launch and in an empty fork;
  - `/fork 2` as an unknown command;
  - `* session: /fork is available once the current turn finishes` under
    the running prompt.
- **The switch** (`forked`, `forked.full`, `forked-first`):
  - the screen and its scrollback reset;
  - `* session forked: <title> (fork)`, then the plain line, then the kept
    turns;
  - the prompt back in the composer, unsent.
- **The window title**: `forked.title` and `fork-continue/continued.title`
  read `first prompt alpha (fork)` and `only prompt omega (fork)` on both.
- **The listing**:
  - from inside the fork, `/resume` lists only the source;
  - after `/new`, it lists both;
  - a zero-turn fork is listed at `1 turn` (`fork-empty/resume`, `resume-all`);
  - a resumed fork says `* session resumed: …` (`resumed-fork`).
- **The exit line** names the fork, a zero-turn fork included
  (`fork-edge/quit`).

## The fork's first request

The request each fork sends first carries only the kept turns and the
refilled prompt (`prompt`, system messages left out):

| scenario | fx | xfx |
|---|---|---|
| `fork` (before `second prompt beta`) | `fx/…-fork-40x120-bodies/005-completion.json`: `user first prompt alpha`, `assistant ALPHA answer.`, `user second prompt beta` | `xfx/…-fork-40x120-bodies/005-completion.json`: the same three |
| `fork-continue` (before the first prompt) | `fx/…-fork-continue-40x120-bodies/003-completion.json`: `user only prompt omega` | `xfx/…/003-completion.json`: the same one |

The source's third turn (`GAMMA answer.`) is in neither. `001-title.json`
on xfx is its session-title request, and not part of this unit.

## The logs

`fx/…-end.files` and `xfx/…-end.files` hold the whole HOME at the end of
each run. Each product writes its own store: fx's v2 `log.jsonl` and xfx's
`events.jsonl` with its `session.json`. The fork's durable shape is P5-FK1's
receipt (`.prd/tui-phase5/receipts/store-fork/`). There is one difference
worth naming. fx appends a `closed` line to the source on the switch, as on
any switch (seq 26 of the research's `p5-effort-fork-v2-source.jsonl`).
xfx writes nothing to the source: its log closes and its lock is released,
as on a `/resume` (`docs/parity.md`, `/fork`).

## Not here

- `/status`'s `Session <id> · saved` for a fork: P-status (D90).
- `/effort`, which shares the research doc and the slash menu: P5-EF1/EF2.
