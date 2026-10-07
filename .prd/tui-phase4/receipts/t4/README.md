# T4 receipt: how a `/clear` is bracketed on the wire

Scenario `scripts/fxcap/scenarios/reset-wire.cap` (turns `reset-turns.json`),
run at 24x80 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  reset-wire 24x80 <dir>
                                  scripts/fxcap/capture.sh xfx reset-wire 24x80 <dir>
```

One prompt and its answer, `wire turn1`, then `/clear` + Enter, then the
screen, the history and `wire clear`. A `.wire` file is every byte the binary
wrote to its pane up to that point (tmux `pipe-pane`, the new `wire` verb), so
the reset's bytes are `clear.wire` minus `turn1.wire`. The xfx files are from
`b99dfa3`; a throwaway xfx run came first, because the first run of a fresh
build is assessed by macOS and starts after the scenario has begun typing.

## The reset's bytes, split at the synchronized-update brackets

From the first block that carries the handoff's linefeeds (the frames before
it are the slash menu while `/clear` is typed). `[CUP+2K xN]` is N rows each
addressed and erased, `[LF x9]` nine linefeeds; colours dropped.

fx:

```
sync      481 bytes  ?2026h ?25l [CUP+2K x15] CUP(24,1) [LF x9] [CUP+2K x24] CUP(1,1) ?2026l
outside    22 bytes  OSC 2 "fx v0.0.13 | work"
sync      445 bytes  ?2026h ?25l [CUP+2K x24] CUP(1,1) <welcome> CUP(3,1) ┃ CUP(5,1) <hint> CUP(3,3) ?2026l
outside     6 bytes  ?25h
```

xfx at `b99dfa3`:

```
sync       57 bytes  ?2026h ?25l CUP(10,1) ED CUP(24,1) [LF x9] ?2026l
outside   136 bytes  CUP(1,1) <welcome> EL CUP(2,1) EL CUP(2,1) EL
sync      126 bytes  ?2026h ?25l OSC 2 "xfx v0.1.0 | work" CUP(3,1) ┃ CUP(5,1) <hint> CUP(3,3) ?2026l
outside     6 bytes  ?25h
```

xfx at `13960f4`, before the change (same scenario):

```
outside   177 bytes  ?25h CUP(10,1) ED CUP(24,1) [LF x9] CUP(1,1) <welcome> EL CUP(2,1) EL CUP(2,1) EL
sync      126 bytes  ?2026h ?25l OSC 2 ... ?2026l
outside     6 bytes  ?25h
```

## What matches, and what does not

- **The handoff is one synchronized update of its own**, cursor hidden on
  entry and left hidden (`?2026l` with no `?25h`), the scrolling linefeeds
  inside it, on both. Upstream: `transcript/runtime.zig:4904-4935`
  (`.synchronized_update`, cursor target `visible = false` at `:4927`),
  `render_engine/terminal_diff.zig:611-616` (movement bytes after `?2026h`),
  `:643-644` (`?25h` only for a visible cursor).
- **The fresh screen is a second update that ends `?2026l` then `?25h`**, on
  both.
- **Not matched: where the welcome line is.** fx paints it inside the second
  update; xfx writes it between the two, as a document append. Every append in
  xfx is its own vector ahead of the band's frame, where upstream's appends are
  movement bytes inside the frame (`terminal_diff.zig:611-616`), so this is
  true of every append, not only this one. On a terminal that honours `?2026`,
  xfx shows the welcome on the blank screen one write before the band.
- Different bytes, same cells: fx erases row by row (`CUP`+`2K`) and erases all
  24 rows again after the scroll; xfx erases with one `ED` before it. Both leave
  a blank screen, and the history above it is the same: rows 2-9 of
  `fx-…-clear.full.txt` and `xfx-…-clear.full.txt` are identical (row 1 is each
  product's banner).

## Scrolling inside a synchronized update

The linefeeds that hand the document to scrollback are inside the block on
both binaries. They still reach scrollback in every implementation checked
here:

- tmux 3.7b (the capture's own terminal): the handed-off turn is in the pane's
  history in both `.full.txt` files.
- libghostty-vt (vendored in herdr): `?2026` pauses only the renderer
  (`renderer/generic.zig:1176-1180`); bytes are applied to the terminal as
  they arrive, and a 1 s timer ends the mode (`termio/Thread.zig:37`,
  `:364-374`).
- fx's own emulator buffers a block and replays its bytes at the `?2026l`
  (`core/terminal/engine.zig:560-596`), so the linefeeds scroll then, in
  order.

No terminal that drops rows scrolled off inside a synchronized update was
found in these sources. That claim is limited to the three implementations
above.
