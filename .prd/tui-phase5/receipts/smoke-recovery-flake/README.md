# smoke-recovery-flake: `3b-shutdown-drain` and `3d-primary-band-recovery`

These two smoke-tui scenarios failed at random on CI. Both causes are in the
harness: a check that reads more, or less, of the wire than the bytes it
claims to describe. The fix changes `scripts/smoke-tui.sh` only. Every number
below comes from the same two binaries, built from `main` @ `93f516b5`.

## The two facts that disagreed

**3d** (`recovery: exactly the frame this build tore and the one it rebuilt (3)`,
`... exactly one frame on this wire ever completed (2)`; CI run 41832130240):

- The scenario counted frame openers and closers over the whole capture.
  It took the capture right after `settled()` saw the rebuild close, and
  expected 2 openers and 1 closer.
- The session always writes one more frame. `failed_band` hands the torn
  attempt back (`src/tui/event_loop.rs:1323`), and `recover_tear` raises
  `pending_recovery` (`:1670`). The next tick, one `TICK` later (8 ms,
  `:82`), runs `force_redraw` (`:1235-1242`) and writes the band again.
  Whether that frame was already in the capture depended on how much the
  harness's last pty read took. In CI it took both frames, so the count was
  3 and 2.
- In the CI capture, frame 3 is an identical full repaint. Replaying that
  capture gives 2 and 1 when the count stops at the rebuild's close, and 3
  and 2 when it also holds the next frame.

**3b** (`and the stream behind it is running`; CI runs 41740485579,
41799433671, and 41797119988's x86_64 job):

- `Trial.wait_for("chunk-0")` searched every byte read so far. `Trial.grid()`
  stopped at the last complete frame. A pty read can end anywhere, so the
  wait returned on a `chunk-0` inside a frame still being written. The grid
  then showed the frame before it: the marker and `Generating`, no chunks.
- Even complete, that frame did not show `chunk-0` on the screen. The pacer
  releases elapsed time times rate (`src/tui/pacer.rs:212`), and `slow-ui` sleeps
  every tick, so the first chunk frame carried chunks 0 to 79. That is 24
  rows, so the same update scrolled both the marker and `chunk-0` into
  scrollback. In the passing run it carried 0 to 39. A replay of the failing
  captures, frame by frame, never shows `chunk-0` on the screen of a complete
  frame. It is always in the scrollback.

The 41797119988 aarch64-linux failure was a unit test
(`clipboard::tests::tool::work_given_up_before_its_tool_starts_never_starts_it`),
not 3b.

The `ConnectionResetError` traceback was printed in the logs of 11 to 16
passing scenarios in every CI run that had one. It is noise from the fixture
server when xfx hangs up, and it does not cause either failure.

## Fix

- `through_closed_frames` is the one cut that both `Trial.peek` and
  `Trial.wait_for` use, so a wait can no longer return on a frame that the
  next grid will not show.
- 3b checks the marker and `chunk-0` against `Grid.document_text()`, the
  screen plus its scrollback, and checks that the marker comes first.
- 3d waits for the frame the tick after a recovery owes, and counts openers
  and closers only through the rebuild's own close. That forced frame must
  be one whole frame right behind the rebuild, with no second cleanup vector
  in between.
- 3d reads the rebuilt screen only up to the rebuild's close, as 3e does,
  because the forced frame rewrites every band row. It then requires the
  forced frame to show the same screen and caret.
- `oracle_test.py` checks every boundary of `through_closed_frames`, in bytes
  and in text.
- `LoopbackHTTPServer.handle_error` no longer prints `ConnectionResetError`
  or `BrokenPipeError`.

## RED and GREEN

`read_shape.py` (here) runs one scenario with every pty read either delayed
(`late:<s>`) or capped (`short:<n>`). Each run had a wall-clock limit and its
own process group, killed at the end. RED uses the helpers from `93f516b5`.
GREEN uses this branch.

| scenario | how it was run | RED | GREEN |
|---|---|---|---|
| 3d | reads as they come | 0/100 | 0/100 |
| 3d | 24 CPU burners on the 18-core machine | 0/100 | - |
| 3d | nice 20, 36 CPU burners | 0/60 | - |
| 3d | `late:0.02` | **18/20**, CI's exact messages | 0/100 |
| 3d | `short:256` | - | 0/100 |
| 3b | reads as they come | 0/50 | 0/50 |
| 3b | `short:256` | **20/20**, CI's exact message | 0/100 |

On this machine, CPU load alone never reproduced either failure. Only
shaping the reads did.

All 37 scenarios, one run each:

| how it was run | `93f516b5` | this branch |
|---|---|---|
| `short:256` | 31/37 | 37/37 |
| `late:0.02` | 36/37 (3d) | 37/37 |

With `short:256`, `93f516b5` also failed 12, 13, 14, 17 and 27. Each read
its own marker from a raw wait and then asserted it on a complete-frame
grid: the same cause as 3b. On this branch all five pass.

The full suite on this branch (`scripts/smoke-tui.sh`, the release binary
and the fault-injection binary) ran 37 scenarios plus the oracle with 0
failures: 1092 checks in the first version, 1105 after the review round. It wrote helpers byte-identical to the ones the loops
ran, and no scenario log carries a traceback.

## Review round (FLAKE-b)

The first version waited for the forced frame and then read the rebuilt
screen from the whole capture. That grid always included the forced frame,
so a rebuild with a wrong row passed. A scratch binary whose rebuild drops
its last row (never committed) shows the gap:

| helpers | 3d against that binary |
|---|---|
| first version (`61760228`) | 0/10 failed: the bad rebuild passed |
| this round | 10/10 failed: hint, band, composer and forced-frame checks |

The new `oracle_test.py` boundary check reports 7 problems for a cut that
searches every byte, 4 for a cut at the first opener, and 0 for the real
cut.

The same table's loops on this round's helpers: 3d with `late:0.02` 0/60,
3d with `short:256` 0/60, 3b with `short:256` 0/60.

## Re-run

```
scripts/smoke-tui.sh <xfx> --faulty <xfx-fi> <evidence>   # writes <evidence>/helpers
python3 read_shape.py <evidence>/helpers late:0.02 3d-primary-band-recovery \
  --binary <xfx> --faulty <xfx-fi> --evidence <dir>
python3 read_shape.py <evidence>/helpers short:256 3b-shutdown-drain \
  --binary <xfx> --faulty <xfx-fi> --evidence <dir>
```
