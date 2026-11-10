---
title: PREVIEW-0.2.0-DATED -- the narrow sweep behind the wrapped-banner reader
type: receipt
tags: [xfx, phase5, preview, fxcap]
---

# PREVIEW-0.2.0-DATED: every snap narrower than the new banner, base against head

The user's request of 2026-11-10 made every version line llmux's
`<version> (<channel> <id>)`, so xfx's welcome line grew from 52 cells
(`x𝒇x v0.1.0-<rev> [debug] · Run /help for commands`) to 72 on a debug
build and 74 on release and preview. At 50 and 60 columns it now takes two
rows, where fx's 35 take one. The sweep's banner rule read one row, and the
only cause that excused a wrapped banner (`xfx-debug-banner-wrap`) named one
scenario. This receipt lists every snap narrower than 74 columns and sweeps
each of them on the base and the head. It shows what the banner rule's new
`wrapped` reading (`scripts/fxcap/cells.py` `join_wrapped`) does to them.

Oracle: fx v0.0.16, executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked by
`sweep.py` before and after each run and by `capture.sh` around each capture.
`capture.sh` starts fx with `FX_AUTO_UPGRADE=0` last in its environment.
Base: `df46777e` (origin/main), debug build, sha256 `5357330e9179…`. Head:
`31ce4320`, debug build, sha256 `f2fea8a66fe9…`. The round-2 commits after it
change no source under `src/` and no `build.rs`. Every heavy run went through
the machine-wide `suite-lock.sh`, slot 4 (`runs.txt` has each run's load).

## Which snaps (`narrow_snaps.py`, `narrow-snaps.txt`)

`narrow_snaps.py` walks each scenario as `capture.sh` runs it for xfx: from
the size the sweep was given, through each `resize`. It prints every snap
taken narrower than 74 columns. The sizes are the ones the sweep and the
receipts use. 40x120 is CI's and every full sweep's. The rest are the units'
own narrow runs: 24x50 for `p5-panels-status-short` (P-status), 40x60 for
`p5-mcp-menu-new` (MCP-M), and 40x50 for five `p5-usage-*` scenarios (#62).
Every other size a receipt names is at least 80 columns. **107 snaps in 9
scenarios** (`narrow-snaps.txt`, last line).

## How they were swept (`narrow-sweeps.sh`, `runs.txt`)

There is one `sweep.sh` per size group: `120`, `50`, `60` and `u50`, once on
each binary. At 50 columns fx's help wraps a slash row that the sweep's
inventory reader refuses. So the 24x50 and 40x50 sweeps take every capture
and then stop with exit 2 before writing a scoreboard (#62's receipt found
the same). `narrow_pairs.py` scores every group's captures with the sweep's
own `score()`: the normalizations, the approved divergences with their
aligned cascade, and the causes. It reads each pair three ways:

- `base`: the base build's captures under this tree's ledger;
- `no-reader`: the head build's captures with the banner rule's `wrapped` off,
  that is, the rule as round 1 left it;
- `head`: the head build's captures under this tree's ledger.

## Result (`narrow-pairs.txt`)

| group | snaps | base pass | no-reader pass | head pass | dropped |
|---|---|---|---|---|---|
| 40x120 `h1-resize`, `p5-transcript-question` | 8 | 2 | 2 | 2 | 0 |
| 24x50 `p5-panels-status-short` | 6 | 1 (+3 approved cascade) | 0 | 1 (+3 approved cascade) | 0 |
| 40x60 `p5-mcp-menu-new` | 34 | 33 (+1 approved) | 0 | 32 (+1 approved) | 1, `list-remote` (below) |
| 40x50 five `p5-usage-*` | 64 | 58 (+4 approved) | 0 | 58 (+4 approved) | 0 |

- **Without the reader, 100 of the 112 pairs drop** (`narrow-pairs.txt`, the
  `pairs:` line): every pair the base passed or had approved in the 24x50,
  40x60 and 40x50 groups. The cause was not a regression in xfx's screen.
  Every row under the wrapped banner stood one lower than fx's.
- **With the reader, base and head agree on 111 of the 112 pairs.** The 40x120
  group's 60-column snaps do not show the banner on xfx's side at all: xfx's
  resized repaint has already pushed it out of the window
  (`captures/` has the 24x50 `esc` pair, where it does show).
- **`list-remote` is the remote MCP server's state at the snap, not the
  banner.** Its one differing row is fx `› remote connecting` against xfx
  `› remote retrying`. In the base's run both said `retrying`, so in the
  head's run it was fx's word that moved, and fx is the same binary in both
  (`captures/*-list-remote.txt`; `retake-words.txt`). train-6's receipt records
  this pair moving between the two words from run to run
  (`.prd/tui-phase5/receipts/train-6/README.md`, the `back-list`,
  `list-remote` row). The re-takes are below.

### The 40x60 re-takes (`retake-60.sh`, `retake-words.txt`, `retake-pairs.txt`)

`p5-mcp-menu-new` ran at 40x60 three more times on each binary, alternating
base and head so both saw the same machine load. With the first take that
makes four per binary:

| take | base: fx / xfx | head: fx / xfx | `list-remote` base, head |
|---|---|---|---|
| 0 (`narrow-pairs.txt`) | `retrying` / `retrying` | `connecting` / `retrying` | pass, unresolved |
| 1 | `connecting` / `retrying` | `connecting` / `retrying` | unresolved, unresolved |
| 2 | `retrying` / `retrying` | `retrying` / `retrying` | pass, pass |
| 3 | `retrying` / `retrying` | `retrying` / `retrying` | pass, pass |

- **xfx says `retrying` in all 8 takes, base and head alike.** Only fx's word
  moves, and the pair is unresolved exactly when fx says `connecting`. The
  base has the same failure (take 1), so this is the oracle's timing, not
  this change.
- **In takes 1-3, base and head give the same verdict on all 34 pairs of
  each take** (`retake-pairs.txt`, the `pairs:` lines), and nothing drops.
  So over the four takes, 0 pairs drop for this change.

## The ledger

`scripts/fxcap/parity-ledger.toml`'s `banner` normalization says
`wrapped = true`. `ledger.py` loads it, `rules_for` passes it on, and
`cells.normalize` joins such lines first, given the snap's screen width:
`sweep.score` passes each side's width at that snap (`capfile.Reading.cols_at`,
the run's size and every `resize` before the snap). Without a width it joins
nothing. A join needs two or three non-blank consecutive rows, read with the
space a word wrap removes or with none, that
- together are a whole match (`fullmatch`, so the contract is the helper's,
  not the pattern's anchors),
- are wider than the screen -- a line that fits is never wrapped -- with no
  row wider than it,
- and break where a wrap breaks: at a word wrap the row, a space and the next
  row's first word are wider than the screen; inside a word, the row is full.

The same banner on two rows of a 120-column screen (a hard line break), rows
broken where the next word would still have fit, rows that only begin like a
banner, rows that make more than one and rows with a blank between them are
left alone (`test_sweep.py`, `test_a_banner_the_screen_wrapped_is_read_as_the_banner`
at 50, 60 and 32 columns, `test_a_banner_broken_where_the_screen_would_not_wrap_it_stays_broken`,
`test_only_a_whole_banner_is_joined`). Taking the break rule or `fullmatch` out
fails a test (`reader-mutants.txt`). Taking out the width bound alone does not,
because the break rule implies it; it stays written out as the contract. With
the bounds, every verdict in `narrow-pairs.txt` and `retake-pairs.txt` is the
same as with round 2's unbounded reader.

A git checkout with no id is refused on purpose (`tests/cli.rs`,
`built_from_a_checkout`): build.rs must name a checkout's build, and CI and
every gate build from a checkout with git, so a checkout without an id is a
broken stamp, not the supported no-git build of `cli-nogit.txt`. The cause `xfx-debug-banner-wrap` is
retired. With the reader on, no pair attributes to it (`narrow-pairs.txt`;
the cause appears only in the `no-reader` column), and its own `fix` said to
read a wrapped banner as the banner. No cause's or divergence's `scenarios`
was widened.

## Files

| file | what |
|---|---|
| `narrow_snaps.py`, `narrow-snaps.txt` | every snap under 74 columns, by run size |
| `narrow-sweeps.sh`, `retake-60.sh` | the sweeps, one per size group, and the 40x60 re-takes |
| `runs.txt` | each sweep's run facts: size, binary sha256s, start, wall time, load |
| `narrow_pairs.py`, `narrow-pairs.txt` | every pair in the three readings, the counts, the drops |
| `retake_words.py`, `retake-words.txt` | the remote server's word on each side, each take |
| `retake-pairs.txt` | takes 1-3 scored base against head: `list-remote` and each take's counts |
| `captures/` | the 24x50 `esc` and 40x60 `list-remote` captures, base and head, both sides |
| `cli-nogit.txt` | `tests/cli.rs`'s version and status tests in a `git archive` tree with no `XFX_BUILD_ID` (MUST 1) |
| `reader_mutants.py`, `reader-mutants.txt` | each bound of `join_wrapped` taken out in turn, and which reader test fails |

The sweep outputs themselves (captures, `pairs.json`, `diffs/`) stay on the
machine that ran them, as every sweep's raw output does. The files here carry
no local path; the commit was checked for one.
