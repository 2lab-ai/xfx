# T8 receipt: a frame's document growth inside its synchronized update

T4 (`../t4/`) found that every document append xfx wrote was a vector of its
own **ahead of** the band's frame, outside any `?2026h … ?2026l`, while fx's
document growth is movement bytes inside the frame's update. This receipt is
the same four kinds of tick on both binaries, before and after the change,
read off the wire.

## How it was taken

```
FX_BIN=<fx c1db919 build, 0.0.13>  scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
XFX_BIN=<xfx>                      scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
```

| Scenario | What it does | Sizes |
|---|---|---|
| `stream-wire` | one prompt; an answer of two paragraphs streamed a few words at a time | 24x80, 12x80 |
| `notice-wire` | a long streamed answer, `/version` typed mid-stream | 24x80, 12x80 |
| `toolgroup-wire` | two steps of read-only tool calls, rewritten in place as each starts and ends, then an answer | 24x80 |
| `reset-wire` (T4's) | one prompt and answer (`turn1`), then `/clear` (`clear`) | 24x80 |

`xfx-before-*` is the release build of `7347b22` (plan `tui-phase4` when T8
began); `xfx-after-*` the release build of `b93b455` (`fb3005c` merged with
plan `8891d04`). Every file is what `capture.sh` wrote: a `.wire` is every byte
the binary wrote to its pane (tmux `pipe-pane`), `.txt` the screen, `.full.txt`
the screen and tmux's history. The `.ansi` copies are left out; the `.wire`
carries the same bytes. Nothing in them names the machine.

`splits.txt` is every wire cut at the synchronized-update brackets by
`wiresplit.py` (`python3 -I wiresplit.py <file>.wire --text`): `sync` is a
`?2026h … ?2026l` block, `outside` everything between two, SGR dropped,
`[CUP+EL xN]` N rows each addressed and erased, text quoted and clipped. Its
first line per file counts the **outside segments that write a cell or
scroll** (text, or a linefeed):

| | stream 24x80 | stream 12x80 | notice 24x80 | notice 12x80 | toolgroup | reset turn1 | reset clear |
|---|---|---|---|---|---|---|---|
| fx | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| xfx before | 4 | 4 | 6 | 7 | 4 | 4 | 5 |
| xfx after | **0** | **0** | **0** | **0** | **0** | **0** | **0** |

What is left outside on every binary is cell-less: the launch's mode set and
queries, the `?25h` that follows each `?2026l`, the window title on fx's side,
and a bell.

## Block by block

Offsets are into the `.wire`; colours dropped.

**A streamed answer's rows** (24x80, `stream-wire`; the document fits, so no
row scrolls):

```
fx      sync   @1163  ?2026h ?25l CUP(5,2) " STREAM-HEAD the firs.." CUP(6,3) "past the right edge o.." … CUP(8,2) " Generating (0s)" … CUP(10,1) "┃" CUP(12,1) "auto" … CUP(10,3) ?2026l
        out    @1517  ?25h
before  out    @811   ?25h [CUP+EL x2] CUP(5,1) "  STREAM-HEAD the fir.." EL CUP(6,1) "  past the right edge.." EL
        sync   @993   ?2026h ?25l CUP(7,1) "• Generating (0s)" … CUP(9,1) "┃" " " EL CUP(11,1) "auto" … CUP(9,3) ?2026l
after   sync   @817   ?2026h ?25l [CUP+EL x2] CUP(5,1) "  STREAM-HEAD the fir.." EL CUP(6,1) "  past the right edge.." EL CUP(7,1) "• Generating (0s)" … CUP(9,1) "┃" " " EL CUP(11,1) "auto" … CUP(9,3) ?2026l
        out           ?25h
```

**The same, scrolling** (12x80; the turn's last rows push the screen up):

```
fx      sync   @1669  ?2026h ?25l CUP(6,64) 0J ?7h CR LF CR LF "  A second paragraph,.." CR LF CR LF "  1s (↑4 ↓40)" ?7l CUP(12,1) LF LF [CUP+2K x12] CUP(1,1) "┃" " " "stream please" … CUP(10,3) ?2026l
before  out    @1597  ?25h [CUP+EL x4] CUP(8,1) "  A second paragraph,.." EL [CUP+EL x2] CUP(12,1) LF CUP(9,1) "  1s (↑4 ↓40)" EL CUP(12,1) LF CUP(9,1) EL
        sync   @1812  ?2026h ?25l OSC2"stream please" CUP(10,1) "┃" " " CUP(12,1) "auto" … CUP(10,3) ?2026l
after   sync   @1664  ?2026h ?25l [CUP+EL x4] CUP(8,1) "  A second paragraph,.." EL [CUP+EL x2] CUP(12,1) LF CUP(9,1) "  1s (↑4 ↓40)" EL CUP(12,1) LF CUP(9,1) EL OSC2"stream please" CUP(10,1) "┃" " " CUP(12,1) "auto" … CUP(10,3) ?2026l
```

(`LF` here is `CR LF` on the wire: the pty's `ONLCR`, on both binaries.)

**A notice mid-stream** (24x80, `/version`):

```
fx      sync   @3444  ?2026h ?25l [CUP+2K x24] CUP(1,1) "𝒇x" … CUP(5,3) "Opening line of the a.." CUP(7,1) "* version: 0.0.13" CUP(9,1) "• Generating (2s)" … CUP(11,3) ?2026l
before  out    @3309  ?25h [CUP+EL x3] CUP(7,1) "* " "version:" " 0.1.0" EL [CUP+EL x2]   (then the band's own block)
after   sync   @3228  ?2026h ?25l [CUP+EL x3] CUP(7,1) "* " "version:" " 0.1.0" EL [CUP+EL x2] CUP(9,1) "• Generating (2s)" … CUP(11,3) ?2026l
```

**A tool group's rewrite**:

```
fx      sync   @1359  ?2026h ?25l CUP(7,3) "4 tool calls · 3 read.." CUP(9,1) "├" CUP(10,1) "├ Matching **/*.txt" CUP(11,1) "└ Searching alpha" … CUP(15,1) "┃" … ?2026l
before  out    @951   ?25h CUP(5,1) EL CUP(5,1) "  I will look at the .." EL [CUP+EL x5] CUP(7,1) "●" " " "1 tool call · 1 read" EL … (907 bytes, four rewrites)
        sync   @1858  ?2026h ?25l CUP(13,1) "• Thinking (0s)" … ?2026l
after   sync   @1750  ?2026h ?25l CUP(11,1) EL CUP(7,1) "●" " " "3 tool calls · 2 read.." EL CUP(9,1) "├ Read b.txt" EL CUP(10,1) "└ Matching **/*.txt" EL … CUP(12,1) "• Thinking (1s)" … CUP(14,3) ?2026l
```

The after run took more ticks for the same turn (23 blocks to 15) on a loaded
machine; every rewrite is inside the update of the tick it landed in.

**A submitted prompt's card** (`stream-wire`, 24x80):

```
fx      sync   @640   ?2026h ?25l CUP(3,3) "stream please" CUP(5,1) "• Thinking" … CUP(7,3) ?2026l
before  out    @346   ?25h CUP(3,1) EL CUP(3,1) "┃" " " "stream please" EL [CUP+EL x2]
        sync   @428   ?2026h ?25l CUP(5,1) "• Thinking (0s)" … CUP(7,3) ?2026l
after   sync   @352   ?2026h ?25l CUP(3,1) EL CUP(3,1) "┃" " " "stream please" EL [CUP+EL x2] CUP(5,1) "• Thinking (0s)" … CUP(7,3) ?2026l
```

**The welcome line after `/clear`** (`reset-wire`, the reset's bytes only):

```
fx      sync   @2309  ?2026h ?25l [CUP+2K x15] CUP(24,1) [LF x9] [CUP+2K x24] CUP(1,1) ?2026l
        out    @2790  OSC2"fx v0.0.13 | work"
        sync   @2812  ?2026h ?25l [CUP+2K x24] CUP(1,1) <welcome> CUP(3,1) ┃ CUP(5,1) <hint> CUP(3,3) ?2026l
before  sync   @2240  ?2026h ?25l CUP(10,1) ED CUP(24,1) [LF x9] ?2026l
        out    @2297  CUP(1,1) <welcome> EL [CUP+EL x2]
        sync   @2395  ?2026h ?25l OSC2"xfx v0.1.0 | work" CUP(3,1) ┃ CUP(5,1) <hint> CUP(3,3) ?2026l
after   sync   @2183  ?2026h ?25l CUP(10,1) ED CUP(24,1) [LF x9] ?2026l
        sync   @2240  ?2026h ?25l CUP(1,1) <welcome> EL [CUP+EL x2] OSC2"xfx v0.1.0 | work" CUP(3,1) ┃ CUP(5,1) <hint> CUP(3,3) ?2026l
```

**The launch** is the same change: before, the welcome line was written
between the queries and the first block; after, it is the first block's
document part, as fx's first block carries its welcome (`@59` after, `@99`
fx).

A **resize repaint** is not on a wire here. It was one update already when no
row was owed; a row owed across it now lands in the same update as the whole
band (`a_resize_repaint_and_the_row_it_owes_are_one_update`). fx repaints the
whole window on a size change (`full_repaint` includes `dimensions_changed`,
`render_engine/terminal_diff.zig:413-416`), inside the one frame.

## The screen did not change

Only the bracketing moved. The `.txt` and `.full.txt` of every T8 scenario,
before against after, are identical except for timer digits a snapshot catches
mid-turn (`5s`/`6s`, `↓61`/`↓65`, `0s`/`3s`). The same comparison over six
earlier scenarios (`streaming`, `notices`, `notice-midstream`,
`toolgroup-noop`, `reset`, `startup`) at 40x120 and 24x80, on the same two
binaries: 48 screens, 45 identical, 3 different only in an output-token count
caught mid-stream. T4's `/clear` screens are identical to the after run's
except for the banner (T4's was a debug build).

## Scrolling inside the update still feeds scrollback

The rows a scroll carries off are now carried by linefeeds inside the update.

- **tmux 3.7b** (the capture's terminal): `xfx-after-stream-wire-12x80-end.full.txt`
  keeps the welcome line, scrolled off by `CUP(12,1) LF` inside the block at
  `@1664`, in the pane's history; every `.full.txt` is the same before and
  after, timer digits aside (above).
- **fx's own emulator** (`core/terminal/engine.zig` at `c1db919`, which buffers
  a block and replays it at the `?2026l`, `:560-596`): `fx-engine-replay.zig`
  feeds the 12x80 wires to it. Before and after scroll the same number of rows
  -- 2 for `stream-wire`, 10 for `notice-wire` -- end with no update open, and
  end on the screen tmux shows, for both xfx wires and for fx's own. That
  emulator keeps no scrollback (`:1701`), so a row's arrival there is not
  modelled; the scroll count is. Its output is `fx-engine-replay.txt`; how to
  run it is at the head of the harness.
- **libghostty-vt** (vendored in herdr): read, not run. Every use of
  `synchronized_output` in `src/` is the renderer skipping a frame
  (`renderer/generic.zig:1176-1179`), the termio thread's 1 s reset
  (`termio/Thread.zig:37`, `:364-374`, `termio/stream_handler.zig:742-743`), a
  resize clearing it (`termio/Termio.zig:492`, `:539`, `terminal/c/terminal.zig:508`),
  and the stream terminal taking the mode with no action
  (`terminal/stream_terminal.zig:507`) -- none on the path a linefeed takes into
  the page list. So bytes inside a block are applied as they arrive.

One bound found on the way: fx's emulator refuses an update past 1 MiB
(`max_sync_bytes`, `engine.zig:24`, `:576`, `SynchronizedUpdateTooLarge`). An
update now carries every row the tick appends, so a multi-megabyte paste
echoed as one card is past it -- as fx's own frame would be, since fx's
movement bytes are inside its update too.
