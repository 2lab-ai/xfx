# flake-mcpmode: a control file read in the moment it was rewritten in place

Registry #19, 20th sighting. PR #82's CI sweep job (macOS arm64,
GitHub-hosted) failed in the harness's own tests, in
`McpHttp.test_hang_holds_until_the_mode_changes`; a rerun on the same head
passed. The cause is in the harness, not in xfx: the test rewrote the mcphttp
mode file in place (truncate, then write), and the fixture's held request
read the file in between, found no word, and answered 500. Lines are at
`main` @ `690aa505` unless marked "now" (the fix, `986b2812`).

Commits: `fd8ddfd1` RED (tests only), `986b2812` the fix, then this receipt.

## The failure

PR #82's sweep job, as quoted in the dispatch:

```
AssertionError: Regex didn't match: 'rpc=ping id=1 session=no version=- mcp-method=- mcp-name=- mode=ready -> 200' not found in
'[mcphttp] listening 49240
[mcphttp] +89ms POST /mcp rpc=ping id=1 session=no version=- mcp-method=- mcp-name=- mode=unknown() -> 500 +891ms
[mcphttp] +891ms POST /mcp rpc=ping id=2 ... mode=ready -> 200 +892ms'
```

`unknown()`, nothing between the parentheses, is the fixture's answer to a
mode file whose first word is the empty string. The same failure, the same
three log lines, came up on its own here on base code while other workers'
gates loaded this Mac (1 run in 9): `natural-base-repro.txt`.

## The path, hop by hop

1. `scripts/fxcap/test_capture.py:61-64` `write()` opens the file `"w"`:
   `O_TRUNC` at open, the text written at close. Between the two the file is
   empty.
2. `test_capture.py:919-926`, the failing test: the server starts in `hang`
   (`McpHttp.server`, `:879-883`); ping id=1 is sent and the client gives up
   after 0.8 s (`socket.timeout`); then `write(self.mode, "ready\n")` (`:923`).
3. `scripts/fxcap/mcphttp.ts:237-251`: the handler reads the mode (`:247`)
   and, while it is `hang`, sleeps 50 ms and reads it again (`:248-251`). Bun
   runs the handler on after the client has gone: id=1 is logged at +891 ms,
   after the 0.8 s timeout.
4. `mcphttp.ts:75-78` `mode()`: `(await Bun.file(modeFile).text()).trim()
   .split(/\s+/)[0]`. For an empty file that is `""` (`[""]`,
   `bun-split.txt`), not in `modes` (`:71`), so it returns `unknown()`.
5. `mcphttp.ts:248`: `unknown()` is not `hang`; the hold ends.
6. `mcphttp.ts:252` -> `answerIn` (`:216-231`): no branch takes `unknown()`,
   the last line (`:230`) answers `500 fixture failing`, and `:254-256` logs
   `mode=unknown() -> 500`.
7. Ping id=2 was sent after `write` returned and read `ready`. The assertion
   (`:926`) looks for id=1 answered in `ready` and finds `unknown()`.

The dispatcher's reading holds. On a quiet Mac the window is microseconds,
and a poll every 50 ms rarely lands in it; on a loaded runner one did.
`mode()` and the hang loop came in with H1 (`32517e22`, 2026-11-03), the test
with `1011b405` (2026-11-04); neither changed since.

## The class, swept

Every file the harness changes while something may read it. "Then" is the
base; "now" is the fix.

| File | Writer | Reader | Then | Now |
|---|---|---|---|---|
| mcphttp mode, from the tests | `test_capture.py` `write()`: `:923` (a request held), `:931`, `:974`, `:984`, `:1009` (none in flight) | `mcphttp.ts:75-78`, polled by `:248-251` | truncate-then-write; an empty read ended a hold with 500: **the flake** | `swap()` (now `:68-77`): a file beside it, `os.replace`; the reader below |
| mcphttp mode, from capture.sh | `mcphttp` verb `capture.sh:959` (temp + `mv`); the launch write `:605` precedes the server | same | already renamed into place | unchanged; now pinned |
| mcphttp mode, the reader | -- | `mode()` | one read; an empty or half-written file was `unknown(...)` | reads again every 10 ms for up to 2 s (`MODE_SETTLE_MS`, now `:76-92`) until the word is a mode; a read error is a retry |
| fakegw releases | `release` verb `capture.sh:772` (`>>`); tests `:738` (`"w"`), `:748` (`"a"`) | `fakegw.ts:434-438` counts `\n`, polled every 20 ms (`:421-431`) | safe by construction: a half line is not counted, so a read under-counts and a hold waits one poll more | renamed into place anyway (now `capture.sh:776`; tests `swap()`), so the rule has no exception; reader unchanged, its reason now written beside it |
| fakegw usage answers | `usage` verb `capture.sh:956` (temp + `mv`), `off` = `rm` (`:955`); launch `:586`; test `:850` (`"w"`, no request in flight) | `fakegw.ts:211-215`: `exists()`, then `text()`, then `JSON.parse` | a `usage off` between `exists()` and `text()` threw (500); a half-written file failed to parse (500) | `settled()` (now `:209-234`): one read, ENOENT = absent, read again up to 2 s until it parses; test `swap()` |
| fakegw second catalog | `models2` verb `capture.sh:961` (temp + `mv`); test `os.rename` (`:787`) | `fakegw.ts:348-350`: `exists()`, then `text()`, served raw | a non-atomic writer could have served an empty catalog | `settled()` (now `:375`) |
| usage clock | `clock` verb `capture.sh:951`: `printf >` in place; the launch write `:664` precedes the binary | the binary: fx `readClockFile` (`src/core/app/usage_host.zig:205-219@481104fd`) and xfx `pinned` (`src/usage/clock.rs:95-114`) -- an unparsable read is the system clock | rewritten in place while the binary may ask the time | renamed into place, owner-only (now `capture.sh:957`); the readers are product code mirroring fx and stay as they are |
| clipboard image | `clipimage` verb `capture.sh:984`: `cp` onto the old file (in place); `off` = `rm` (`:983`) | fake osascript `capture.sh:686-689`: `[ -f ]`, then `cat > out` | an in-place copy could be read half written; an `off` between `-f` and `cat` left an empty image file | temp + `mv` (now `:990`); the shim `cp`s (now `:689`), which writes nothing if the image is gone |
| `clipboard-fail` | `clipfail` `: >` / `rm` (`:1011-1012`) | shims' `[ -f ]` | a flag: create and unlink are atomic | unchanged |
| `binpid` | `jobctl.pl:41` (`open '>'`, print) | `binary_pid`, `capture.sh:401-430` | the reader asks 20 times and takes a pid only if its parent is the pane and its executable the binary | unchanged |
| `until`, `launched` | `: >` | `since` (`:432-434`), the mtime only | no content read | unchanged |
| `turns.json`, seeded HOME files, `githead`, `pnghdr`, `sparsepng` | `capture.sh:581`, `:632-651`, `:511-528` | the gateway and the binary | written before the server or the binary starts | unchanged |
| `clipboard`, `clipboard-file` | the shims (the binary's side) | `snap`, `clipfile` (`:881-882`, `:989-991`) | read by the harness after; `mv` keeps the shim's inode | unchanged |
| sweep job status | `sweep.py:225` | the sweep's report | written after the job ends | unchanged |
| p5 stdio fixtures' modes | argv (`p5-mcp-discover-stdio.ts:52`) | -- | no file | unchanged |
| `smoke-tui.sh` files | `:3473` notes before launch; `:3614`, `:3648`, `:4989`, `:5282` evidence | -- | not read while written | unchanged |

## RED, deterministic (`fd8ddfd1`)

Three tests, each failing on the base on every run (`red.txt`):

- `McpHttp.test_a_held_request_reads_past_a_mode_file_caught_empty`: a held
  request; the mode file is truncated and **left empty for 400 ms** (8 polls
  at 50 ms), then `ready` is swapped in. Base: `mode=unknown() -> 500`.
- `FakeGateway.test_a_usage_file_or_second_catalog_caught_empty_is_read_again`:
  each file is empty when the request comes and filled 400 ms later. Base:
  usage `500 fakegw: SyntaxError: JSON Parse error: Unexpected EOF`; second
  catalog `200` with an empty body.
- `Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole`: the stub
  shell opens a descriptor on each of the six files a verb changes mid-run,
  the verbs run, and the shell reads each descriptor and each path. A rename
  leaves the descriptor on the old file; an in-place write shows it the new
  bytes. Base: exactly `clock`, `release` and `clipimage` fail, the three the
  sweep found rewritten in place; `usage`, `mcphttp` and `models2` pass. Each
  step waits for the shell's answer on the screen (`until shown`), not for a
  moment on the clock.

The fix's bounds (2 s) are 5x the RED's empty window (400 ms).

## Loops

The affected tests: the `McpHttp` and `FakeGateway` classes and the six
`Verbs` tests of the changed verbs (`clock`, `usage`, `models2`, `mcphttp`,
`held`/`release`, and the new one), 27 tests a run (`loop.sh`). Each loop ran
from a frozen export of its commit, under the suite lock; the per-test
counts and every run's summary line are in the `.counts` files.

| Loop | Commit | Load | The 3 RED tests | `test_hang_holds_until_the_mode_changes` | The other 23 |
|---|---|---|---|---|---|
| `before-red.counts` | `fd8ddfd1` (RED) | none added | 0/20 each | 20/20 | 20/20 each |
| `after.counts` | `986b2812` (fix) | none added | 20/20 each | 20/20 | 20/20 each |
| `after-load.counts` | `986b2812` (fix) | one `yes > /dev/null` per core, 18 (`load.sh`); load average 34-54 | 20/20 each | 20/20 | 20/20 each |

The flaked test on base code under the machine's own load is in
`natural-base-repro.txt`: an earlier loop of the RED commit's first form
(same base harness, the RED Verbs test still on fixed waits) failed it once
in 9 runs, with CI's three log lines. A further loop of the flaked test alone
on base under the spinners was stopped before its first run: every lock slot
was taken by other workers' gates, and the spinners were loading those gates
while it waited.

## Mutants

Ten half-reverts of the fix, each on a throwaway copy of `986b2812`'s
`scripts/fxcap` (`mutants.py`): 10/10 killed (`mutants.txt`). They include
the mcphttp reader with no retry and with a 0 ms bound, fakegw's
take-it-at-once and old `exists()`-then-`text()` second catalog, ENOENT as a
failed read (killed by the existing `usage off` tests), and each of the five
renames put back in place.

## Gate

GATE

## Files

- `natural-base-repro.txt` -- base code failing as on CI, unprovoked.
- `bun-split.txt` -- `"".trim().split(/\s+/)` is `[""]`; a missing file's
  read rejects with `ENOENT`.
- `red.txt` -- the three RED tests on the RED commit.
- `loop.sh`, `load.sh`, `*.counts` -- the loops and their pass counts.
- `mutants.py`, `mutants.txt` -- the half-reverts.
- `gate.txt` -- the CI steps.
