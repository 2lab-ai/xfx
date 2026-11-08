# flake-mcpmode: a control file read in the moment it was rewritten in place

Registry #19, 20th sighting. PR #82's CI sweep job (macOS arm64,
GitHub-hosted) failed in the harness's own tests, in
`McpHttp.test_hang_holds_until_the_mode_changes`; a rerun on the same head
passed. The cause is in the harness, not in xfx: the test rewrote the mcphttp
mode file in place (truncate, then write), and the fixture's held request
read the file in between, found no word, and answered 500. Lines are at
`main` @ `690aa505` unless marked "now" (the last code commit, `29fb72d8`).

Commits, in order:
- `fd8ddfd1` RED: the three tests below, failing on base.
- `986b2812` the fix: mcphttp's and fakegw's readers; `clock`, `release` and
  `clipimage` renamed into place; the tests' `swap()`.
- `4647fd7d` this receipt, first form.
- `760d7f85` RED, second pass of the sweep: `reseed` and `rehomefile`, and a
  HOME path that is a link.
- `29fb72d8` the fix for it: `put_in_home`.
- then this receipt in its final form and the gate.

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
| fakegw releases | `release` verb `capture.sh:772` (`>>`); tests `:738` (`"w"`), `:748` (`"a"`) | `fakegw.ts:434-438` counts `\n`, polled every 20 ms (`:421-431`) | safe by construction: a half line is not counted, so a read under-counts and a hold waits one poll more | renamed into place anyway (now `capture.sh:793`; tests `swap()`), so the rule has no exception; reader unchanged, its reason now written beside it |
| fakegw usage answers | `usage` verb `capture.sh:956` (temp + `mv`), `off` = `rm` (`:955`); launch `:586`; test `:850` (`"w"`, no request in flight) | `fakegw.ts:211-215`: `exists()`, then `text()`, then `JSON.parse` | a `usage off` between `exists()` and `text()` threw (500); a half-written file failed to parse (500) | `settled()` (now `:209-234`): one read, ENOENT = absent, read again up to 2 s until it parses; test `swap()` |
| fakegw second catalog | `models2` verb `capture.sh:961` (temp + `mv`); test `os.rename` (`:787`) | `fakegw.ts:348-350`: `exists()`, then `text()`, served raw | a non-atomic writer could have served an empty catalog | `settled()` (now `:375`) |
| usage clock | `clock` verb `capture.sh:951`: `printf >` in place; the launch write `:664` precedes the binary | the binary: fx `readClockFile` (`src/core/app/usage_host.zig:205-219@481104fd`) and xfx `pinned` (`src/usage/clock.rs:95-114`) -- an unparsable read is the system clock | rewritten in place while the binary may ask the time | renamed into place by `put_in_home` (now `:438-451`, `:976`), owner-only; the readers are product code mirroring fx and stay as they are |
| the binary's settings, `reseed` | `reseed` verb `capture.sh:911-914`: `>` in place, then `chmod 600` | the binary, when it reads its settings | documented for "between an exit and a `relaunch`", but nothing holds a scenario to that; its one use (`p5-session-early-prefs.cap:29`) follows a `/quit` by 2 s | `put_in_home` (now `:932-937`), 0600 |
| a HOME file, `rehomefile` | `rehomefile` verb `capture.sh:916-923`: `>` in place ("an existing file is written in place", its doc), then `chmod 600` | the running binary: "a settings file made malformed after launch" | rewritten in place under a running binary; and a HOME path that was a link was written through, to wherever it pointed (`confinement-red.txt`) | `put_in_home` (now `:938-946`), 0600; a link or a directory there is refused, exit 2 |
| `put_in_home`'s refusal | -- | -- | -- | `mv` onto a link to a directory would move the file into that directory, so the helper refuses a link or a directory outright. It checks the leaf only: a `rehomefile .fx/settings.json` whose `~/.fx` is itself a link to a directory outside still lands there, as it did on base (`mkdir -p` and `>` follow it too); scenarios are the repo's own, and an intermediate link is left to the seeding confinement checks, not widened here |
| clipboard image | `clipimage` verb `capture.sh:984`: `cp` onto the old file (in place); `off` = `rm` (`:983`) | fake osascript `capture.sh:686-689`: `[ -f ]`, then `cat > out` | an in-place copy could be read half written; an `off` between `-f` and `cat` left an empty image file | temp + `mv` (now `:1009`); the shim `cp`s (now `:689`), which writes nothing if the image is gone (driven end to end: `image-paste.txt`) |
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

The fix's bounds (2 s) are 5x the RED's empty window (400 ms). No committed
scenario can meet the new 2 s re-read of fakegw: all 11 files a `usage` or
`models2` line names parse as JSON (9 scenarios checked with `json.load`).

## RED, second pass (`760d7f85`)

The sweep's second pass found `reseed` and `rehomefile`, which write the
binary's HOME files with `>`:

- the descriptor test now opens those two files too: on `4647fd7d` both fail
  (`settings.json` and `.fxcap-rehome` read the new bytes through the old
  descriptor), the other six pass; on base, five fail -- the five verbs that
  wrote in place;
- `Confinement.test_a_home_file_a_verb_replaces_is_never_a_link_or_a_directory`:
  a HOME path that is a link to a file outside the run, and one that is a link
  to a directory outside. On `4647fd7d` (and base) the first is written through
  -- exit 0, the file outside overwritten -- and the second fails the run with
  `Is a directory` (exit 1). Both must be refused (exit 2) with nothing outside
  changed. `confinement-red.txt`.

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
| `after-load.counts` | `986b2812` (fix) | one `yes > /dev/null` per core, 18 (`load.sh`); load average 34-54; two other suites running (below) | 20/20 each | 20/20 | 20/20 each |

The second pass's four tests (the descriptor test, the link test,
`test_clock`, `test_rehomefile`), 20 runs each way under the lock:
`before-red2.counts` on `760d7f85`, the descriptor and link tests 0/20 and
the other two 20/20; `after2.counts` on `29fb72d8`, all four 20/20; and
`after2-load.counts`, the same on `29fb72d8` under the spinners, all four
20/20. The
27-test loops above ran on `986b2812`; what `29fb72d8` changes after it is
`reseed`, `rehomefile`, `clock` and the two tests, all in the second-pass
loop, and the gate's full `test-capture.sh` (63 tests) ran on it.

**The loaded runs overlapped other workers' suites.** Each loaded loop
held one lock slot, but the spinners loaded every core, so the two suites in
the other two slots ran under that load too, and the loaded counts were
taken while they ran (machine load ~29-54):
- `after-load.counts`: spinners 2026-11-08 15:06:49Z-15:25:43Z, the loop
  15:07:07Z-15:22:16Z; the other slots held GRID-ROW-COST's sweep (from
  15:11:51Z) and train 7's gate. The spinners ran on 3.5 min past the loop,
  while a base-only loop waited for a slot and never started.
- `after2-load.counts`: spinners 16:34:49Z-16:42:16Z, the loop
  16:35:32Z-16:42:16Z; the other slots held GRID-ROW-COST's gate and train
  7's head sweep.

`load.sh` is what ran. A run that loads every core must hold every slot
instead -- the lock nested once per slot (`suite-lock.sh suite-lock.sh
suite-lock.sh <cmd>` with the default three) -- and stop its spinners in
that command's own EXIT trap.

The flaked test on base code under the machine's own load is in
`natural-base-repro.txt`: an earlier loop of the RED commit's first form
(same base harness, the RED Verbs test still on fixed waits) failed it once
in 9 runs, with CI's three log lines. A further loop of the flaked test alone
on base under the spinners was stopped before its first run: every lock slot
was taken by other workers' gates, and the spinners were loading those gates
while it waited.

## Mutants

Fourteen half-reverts, each on a throwaway copy of `29fb72d8`'s
`scripts/fxcap` (`mutants.py`): 14/14 killed (`mutants.txt`). They are the
mcphttp reader with no retry and with a 0 ms bound; fakegw's
take-it-at-once, its old `exists()`-then-`text()` second catalog, and ENOENT
as a failed read (killed by the existing `usage off` tests); each of the
seven renames put back in place (`clock`, `reseed`, `rehomefile`, `release`,
`clipimage`, `usage`, `mcphttp`); and `put_in_home` with no refusal and
refusing a directory only. Not pinned by a mutant: the fake osascript's `cp`
(its window, an `off` between `[ -f ]` and the read, needs a paste in flight
at that moment); `image-paste.txt` drives it end to end instead. And the
tests' own `swap()`: reverting it to `write()` leaves the readers to catch
the empty file, which they do.

## Gate

Every CI step, at `7266e29d` (the code of `29fb72d8` plus receipt text),
the heavy ones under the suite lock (`gate.sh`; `gate.txt` has each step's
exit status and count): fmt, clippy and clippy with fault injection, `cargo
test` (5327 passed, 0 failed, 7 ignored) and with fault injection (5364
passed, 0 failed, 7 ignored), all six `scripts/check-*.sh` and three
`scripts/test-*.sh`, `fxcap/test-sweep.sh` (66 + 26), `fxcap/test-capture.sh`
(63), the release and fault-injection builds, `smoke.sh` (47 checks) and
`smoke-tui.sh` (37 scenarios, 1168 checks): all exit 0. The plain `cargo
test` step's 2200 s is mostly the wait for a lock slot.

## Files

- `natural-base-repro.txt` -- base code failing as on CI, unprovoked.
- `bun-split.txt` -- `"".trim().split(/\s+/)` is `[""]`; a missing file's
  read rejects with `ENOENT`.
- `red.txt` -- the three RED tests on the RED commit.
- `confinement-red.txt` -- the second pass's RED, on the first fix and on
  base, and the write through a HOME link shown by hand.
- `image-paste.txt` -- the fake osascript's `cp` driven end to end, fx and
  xfx.
- `loop.sh`, `load.sh`, `*.counts` -- the loops and their pass counts.
- `mutants.py`, `mutants.txt` -- the half-reverts.
- `before-red2.counts`, `after2.counts`, `after2-load.counts` -- the second
  pass's loops.
- `gate.sh`, `gate.txt` -- the CI steps.
