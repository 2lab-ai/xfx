# xfx — TUI port

Status: **Phases 1 and 2 of the MVS ladder below are in the binary. Phase 3 and the explicitly
deferred list are still the target.** The line-oriented shell is unchanged beside it and still never
enters raw mode: it is what a bare `xfx` runs without `XFX_TUI=1`, and `docs/parity.md`'s
`interactive` row is its contract. The shipped TUI's contract is that file's `full-screen TUI` row,
which is the one to read against the code; what this document keeps is the upstream evidence and the
reasoning behind each decision, which a ledger row cannot carry. Where the two disagree the code is
right and both files are bugs.

Evidence base: [`research/tui-core.md`](research/tui-core.md) and
[`research/input-footer.md`](research/input-footer.md), both read against a local clone of upstream
`fx` at **HEAD `ef1d0d0`** — a later commit than xfx's pin `580a0c5d`. Every `file:line` below is
upstream-relative and comes from those notes; `[추정]` marks are theirs and are preserved.

## The one decision that matters

fx's main UI is **not an alternate-screen application.** It owns a band at the bottom of the *normal*
buffer. Completed transcript rows are appended to the terminal's own document and live forever in
native scrollback; fx repaints only its band (transcript viewport + activity row + footer/composer).
Alt screen is an exceptional, exclusively-owned state for modals — the owner is an enum,
`AlternateScreenOwner = { none, file_approval, full_transcript, catalog_menu, subagent_manager,
terminal_session }` (`src/ui/shell_runtime.zig:57-64`), and it is `none` in normal operation. The
normal exit restore sequence does not even contain `1049l`, because the main surface was never there
(`app_lifecycle.zig:39-41`).

This is why xfx can adopt the TUI **without giving up the property it currently sells.** Today's
guarantee is "scrollback survives, and there is no terminal state to restore." Upstream's model keeps
the first half by construction and converts the second half into an obligation: raw mode plus an
exact restore path. Everything below is written to make that obligation testable.

## Target model

### Terminal ownership

- **Raw-mode lifecycle** (`app_lifecycle.zig:447-530`): isatty check → capture original `termios` →
  raw mode (BRKINT/ICRNL/INPCK/ISTRIP/IXON/IXOFF off, CS8, ECHO/ICANON/IEXTEN/**ISIG** off, VMIN=1
  VTIME=0) → install SIGWINCH → install SIGTERM/SIGHUP abnormal-exit handlers. SIGINT is deliberately
  *not* among them: with ISIG off the terminal does not generate it.
- **Shutdown** (`:578-593`): leave any alt screen → restore signal handlers → write the restore
  sequence → restore `termios` → move to the footer top and `\x1b[J\x1b[?25h\n`, leaving the
  transcript in scrollback.
- **Abnormal exit** writes one compile-time-constant restore string with a single async-signal-safe
  `write(2)` (`:53-68`). A **`SIGTSTP`** restores cooked mode, raises the stop with the disposition
  reset, and on SIGCONT re-captures `termios`, re-enters raw, re-queries layout, and requests a full
  repaint (`:609-620`, `:646-656`). It is a **signal, not a keystroke**: raw mode clears `ISIG` (the
  entry bits above), so a *typed* Ctrl-Z — like a typed Ctrl-C — generates no signal at all and
  arrives as a byte for the input decoder, which binds it to nothing. The stop this path answers is
  the one an operator or a supervisor sends, and in xfx it is either **blocked** or delivered
  **inside** `signals::wait_for_input` (`pselect(2)` carrying the mask), which is what keeps a stop
  from landing while the session believes a cooked terminal is raw. `docs/parity.md` states the same
  contract.
- **Scrollback preservation** has three mechanisms, all load-bearing:
  1. At launch, query the cursor row (CSI `6n`, 100 ms deadline, `shell_runtime.zig:178-206`) and push
     the existing shell output above it into scrollback (`app_lifecycle.zig:556-583`). The push is two
     steps and both are load-bearing: **move the cursor to the bottom row first** (`CUP(rows, 1)`),
     *then* emit one literal `\n` per row that was above the cursor. A linefeed scrolls a terminal only
     when the cursor is already on the bottom margin — anywhere else it merely walks the cursor down
     and the screen does not move — so a burst emitted from where the shell left the cursor displaces
     `max(0, 2r - R - 1)` rows instead of `r - 1`, which is nothing whatsoever from row 2 of a 24-row
     screen, and the band opens on top of output that is still there. Same sequence as mechanism 2
     below, which is not a coincidence: it is the one way a terminal is made to scroll.
  2. During the session, rows that leave the viewport are written to the document — CR-before-LF
     normalized bytes (`frame_scroll_plan.zig:8-12`, `transcript/painter.zig:2720-2752`) appended with
     autowrap temporarily on (`terminal_diff.zig:1348-1397`); the scroll itself is CUP-to-bottom plus
     literal `\n`, so the terminal really scrolls and the row really enters native scrollback.
  3. **Mouse reporting is never enabled on the main surface**, so the user's wheel is the terminal's
     own scrollback. Upstream pins this as a contract in a test that asserts `1000h/1002h/1006h` are
     absent (`terminal.zig:135-142`).
- **Escape inventory** to port (`terminal.zig:4-13`, `app_lifecycle.zig:36-44`): modifyOtherKeys
  `\x1b[>4;2m`, kitty keyboard push `\x1b[>1u` (**omitted under tmux** — pushing it there breaks key
  input, `terminal.zig:29-34`), bracketed paste `?2004h`, autowrap off `?7l`, synchronized output
  `?2026h/l` around every frame, OSC 2 title, OSC 11 background query, theme-change notification mode
  `?2031`, and SGR mouse **only** on alt-screen surfaces.

### Frame model

- **A frame is a cell grid, not an array of lines.** `FrameSurface` is a `FrameCell` array with
  interned hyperlinks and combining suffixes, initialized *from the shadow grid*
  (`frame_surface.zig:11-29,116-190`), with a per-cell owner policy on write (`:236`).
- **Layout** is `rows/cols/content_bottom/divider/input/hint` (`terminal.zig:47-59`);
  `frame_layout.solve` places transcript/footer/activity (`frame_layout.zig:156-300`) and
  `footer_layout.resolve` places rows inside the footer (`footer_layout.zig:3-39`). Because footer
  height and transcript occupancy are mutually dependent, upstream converges candidates with a
  **fixed-point iteration** (`app_render_runtime.zig:3294-3420`).
- **A shadow VT is the single source of truth for what is on the terminal.** A bounded in-process
  engine (`src/core/terminal/engine.zig:1-3,222,489`) is fed *every byte xfx writes*
  (`app_lifecycle.zig:1069-1080`), and the frame commit diffs the target surface against it
  (`:490-492`). The footer subsystem states the consequence explicitly: "there is no private prev-row
  state" (`footer/viewport.zig:90-95`).
- **Commit pipeline** (`frame_builder.zig:70-134`): validate the paint plan and add invalidations →
  build terminal-movement bytes (alt→normal transition, document append, alignment scroll) and feed
  them to a shadow clone to predict the post-movement grid (`terminal_diff.zig:1213-1300`) → init the
  surface from that shadow and let the body/footer/activity painters write into it → `flushFrame`.
- **`flushFrame`** (`terminal_diff.zig:335-560`) counts changed cells and **skips a no-op frame
  entirely**; otherwise `composeWireFrame` emits `?2026h` → `?25l` → movement → per-invalidation-range
  `Grid.diffBand` → CUP → `?2026l` → `?25h` (`:580-631`). `diffBand` (`engine.zig:2191+`) re-emits only
  the changed span of each row, tracks SGR state to emit minimal transitions, handles OSC 8
  transitions, and pre-erases wide-glyph overlap with `\x1b[{n}X`. After the write it **feeds its own
  bytes back into a shadow clone and compares**; a mismatch or partial write is a failed commit with a
  retry invalidation (`:633+`, `frame_builder.zig:340-343`).

### Event loop

- One thread, `poll(2)` on stdin only (`shell_runtime.zig:257-278`), **8 ms fixed tick**
  (`src/main.zig:183`), no timer fds — deadlines are millisecond comparisons on that tick.
- Each iteration: `collect_facts` → drain bytes deferred by a probe → `pollInput(timeout)` → burst
  read capped at **32 × 128 B** so sustained input cannot starve rendering → per-byte `handle_byte` →
  `settle_delivery_epoch` → `commit_frame` (`event_loop.zig:76-122`, cap at `:16`).
- **Every non-stdin input is polled in `collect_facts`** (`main.zig:2494-2590`): theme monitor,
  resize (SIGWINCH sets an atomic bit only — `resize_runtime.zig:46-90` — debounce and cursor probe
  happen here), escape-timeout flush, the agent/tool event drain, and the pacer tick.
- **Rendering is request-based**, not per-iteration: a `RenderRequestState` accumulates typed reasons
  (`first_frame, transcript, footer, modal, subagent_panel, animation, notification, resize,
  external_damage`) and invalidations (`render_request.zig:5-16`); a commit with nothing pending is
  skipped, and a failed or input-preempted attempt is restored (capped at 4 consecutive aborts,
  `:69`).
- **Animation** is a 50 ms interval with a 40-frame phase cycle, and the blink half-period is forced
  to line up with 500 ms of wall clock at compile time (`render_request.zig:64-84`).

### Streaming UX

- **The pacer is the reason streamed text feels like fx.** Deltas are enqueued, and each tick emits
  `elapsed × cps` bytes with `cps = clamp(backlog / 1.5s, 400..5000)` and a 200 ms drain target after
  the turn ends (`assistant/pacer.zig:110+,312-318`). ANSI sequences are emitted atomically — an
  incomplete tail waits for the next tick — and an `SgrState` tracks open attributes so that when
  another painter writes `\x1b[0m` between frames, the next emission **re-opens** them (`:40-108`,
  and upstream's own warning comment at `:116-120`).
- **Activity row**: `"• Thinking"` + elapsed + tokens (`activity_status.zig:26-33`), with the clock
  **frozen while an approval or question is pending** (`:37-40`) and a 500 ms blink; the shimmer
  position comes from the animation phase.

### Theme

Start-up detection: `FX_THEME`-equivalent env override → OSC 11 query (200 ms deadline) → `COLORFGBG`
→ default dark; luminance > 32768 means light (`theme_detection.zig:22-62`, `theme_protocol.zig:11-40`).
Truecolor is gated on `COLORTERM` with Apple Terminal downgraded (`:44-53`). Live re-tinting
(mode 2031 + DSR `?996n`) additionally rewrites stored SGR in the transcript and patches the pacer's
pending buffer (`app_render_runtime.zig:343-357`) — deferred, see the ladder.

## Runtime topology (authoritative)

This section is the single source of truth for thread and runtime ownership under the TUI.
[`02-architecture.md`](02-architecture.md) §"Concurrency and process model" describes v0.1.0 and
points here for the target.

**What ships today**, read from the code rather than remembered:

- One **current-thread** tokio runtime, built in `main` and driving everything through one
  `block_on(app::run(...))` (`src/main.rs:17-25`). The comment states the reason: xfx drives one turn
  at a time and never spawns work that outlives the command.
- One extra **detached OS thread**, `xfx-interrupt`, with its own current-thread runtime, for SIGINT
  only (`src/app.rs:539-580`). It exists precisely because the main runtime is single-threaded and is
  *blocked* for the duration of a `terminal` command and for as long as a user takes to type a line.
  Its start is a handshake, not a hope: a `sync_channel(1)` releases the caller only once the signal
  future has been polled once (that poll is the registration), bounded by a 2 s timeout, and a signal
  that arrives inside the install window is still delivered.
- The shell's line read is a **blocking `std::io::stdin().read_line`** on the runtime thread
  (`src/interactive.rs:602-604`). Turn/idle state is a `Mutex<Activity>` (`:254-266`) that the signal
  thread inspects to decide whether Ctrl-C means cancel, clear, or exit 130 (`:303-306`).

**What the TUI changes, and what it must not.** The event loop is a blocking `poll(2)` on stdin with
an 8 ms tick, and a turn is an async stream on the tokio runtime. Those cannot both own the same
thread. The topology is therefore:

```
UI thread (= main thread)
  raw mode · poll(2)+8ms tick · escape decode · frame commit · signal-flag observation
  owns the terminal exclusively; owns the CancellationToken (+ its atomic mirror); owns panic-restore
        ▲                        │                        │
        │ UiEvent                │ TurnControl            │ TurnWork
        │ bounded ~256           │ UNBOUNDED              │ capacity 1
        │ send().await           │ approval answer,       │ submit prompt,
        │ (never blocking_send)  │ cancel, shutdown       │ /model change
        │                        ▼ drained mid-turn       ▼ full ⇒ UI rejects w/ notice
  runtime thread: current-thread tokio · one turn at a time
                  Provider::stream + tools + session writes
                  panics caught at the task boundary → fatal UiEvent

  signal handlers (process-wide, async-signal-safe only):
    INT/TERM/HUP → restore pair → SIG_DFL → re-raise
    TSTP         → restore pair → SIG_DFL → raise(SIGSTOP)
    CONT         → set atomic resume flag + self-pipe byte  (no work in handler)
    WINCH        → set atomic flag + self-pipe byte

  no xfx-interrupt thread in a TUI session (it remains for `ask` and the pipe paths)
```

- **The UI thread is the main thread**, and it owns the terminal exclusively. Nothing else writes a
  byte to stdout: the pacer, the painters and the document-append path all run here. This is what
  makes the shadow grid's invariant — "the shadow is the single source of truth for what is on the
  terminal" — provable rather than aspirational, since a second writer would desynchronize it.
- **The runtime moves off the main thread** into a named worker owning the current-thread runtime.
  It stays current-thread: still one turn at a time, still no work outliving the command.
- **Signal ownership is split, and each signal has exactly one owner** (see §"Signals" below).

**Channels: one work channel, one control channel, one event channel.** Two review findings are fixed
here by structure rather than by tuning: a single capacity-1 FIFO carrying submits *and* approval
answers deadlocks a turn waiting for an approval it cannot dequeue, and a bounded event channel plus a
joining UI deadlocks a producer parked on a full event channel.

| Channel | Direction | Capacity | Carries | Full/closed behavior |
|---|---|---|---|---|
| `UiEvent` | runtime → UI | bounded (~256) | text delta, tool start/result, usage, terminal event | producer **awaits** `send()` inside a `select!` against cancellation — **except the terminal event, whose send is uncancellable** (below); **never dropped while the UI is live**. After the terminal event, see the drain protocol |
| `TurnControl` | UI → runtime | **unbounded** | approval answer, cancel, shutdown | never blocks, never full; consumed **mid-turn** |
| `TurnWork` | UI → runtime | 1 | submit prompt, `/model` change | full ⇒ UI **rejects with a visible notice**; never silently dropped |

- **The producer awaits; it must not `blocking_send`.** The turn runs *inside* a current-thread tokio
  runtime, and `mpsc::Sender::blocking_send` **panics** when called from within a runtime context
  rather than backpressuring — it is the API for a plain OS thread, not for async code. The correct
  shape is an awaited send that can also observe cancellation, so a full channel parks the task (not
  the thread) and a cancel still lands:

  ```rust
  // `cancel: tokio_util::sync::CancellationToken`
  tokio::select! {
      biased;
      _ = cancel.cancelled() => return Err(Cancelled),
      result = events.send(event) => result.map_err(|_| UiGone)?,
  }
  ```

  `biased` makes cancellation win a tie, so a turn cancelled while parked on a full channel exits
  immediately instead of waiting for a slot. Awaiting also yields the runtime, which is what lets the
  same thread keep polling the socket and the control channel while the UI catches up — backpressure
  reaches the decode, which is the effect we wanted, and `blocking_send` would have crashed before
  producing it.
- **Exception: the terminal `UiEvent` send is uncancellable.** It is the worker's acknowledgement that
  the drain may end (step 3 of the shutdown protocol), so selecting it against cancellation would be
  circular — the common reason to send it *is* that cancellation fired, and a `biased` cancel branch
  would win every time and the UI would never receive the acknowledgement it is waiting for. The
  terminal send therefore uses a plain `events.send(event).await`, with three properties making that
  safe: the UI keeps its receiver alive and keeps draining precisely so this send completes; the send
  is bounded by the UI's drain **deadline** rather than by cancellation; and if the deadline expires
  the UI drops the receiver, at which point the send resolves `Err` immediately and the worker exits.
  This is the one send in the system that does not go through the `select!` helper above, and it must
  be written so that the exception is visible at the call site rather than inferred.
- **If any genuinely synchronous producer remains** — something outside the runtime, on its own std
  thread — it uses a **sync bridge** (`blocking_send` is correct *there*, since there is no runtime
  context on that thread) and nothing else changes. As specified, no such producer exists: every
  `UiEvent` originates inside the turn.
- **`TurnControl` must be separate and unbounded.** It is consumed by the runtime *while a turn is in
  flight* — an approval answer is by definition something the turn is blocked on. Putting it behind
  the same capacity-1 slot as a submit is a self-deadlock: the turn waits for an answer queued behind
  a prompt that cannot be dequeued until the turn ends. Unbounded is safe because its producers are
  human keystrokes — at most one pending decision plus a cancel.
- **`TurnWork` overflow is retained-or-rejected, per case.** One turn at a time is the product's
  model, so a submit arriving mid-turn is *queued* (capacity 1) and the composer renders `queued N`.
  A **second** submit while one is already queued is **rejected at the UI**, before the send, with a
  one-line notice on the hint row, and the text stays in the composer so nothing typed is lost.
  Rejection is a UI decision precisely so it can be shown — a `try_send` that failed inside the
  runtime would be a prompt xfx swallowed.
- **Runtime dequeue order during a turn**: `TurnControl` is polled at every await point in the turn
  loop and drained fully before `TurnWork` is looked at at all. Between turns: control first, then at
  most one work item.

**Cancellation is one design with two faces, and the awaitable one is the primitive.**

- **`tokio_util::sync::CancellationToken` is the primitive.** It is the thing selected on in `select!`,
  because an `AtomicBool` cannot be awaited: a task parked on a full channel or a quiet socket needs a
  future that *wakes* it, and polling a flag between awaits cannot do that. Every async cancellation
  point in the turn — the producer helper above, the tool loop, the HTTP read — takes a clone of this
  token. (`tokio-util` is a new dependency; the alternative pairing of `tokio::sync::Notify` with the
  existing flag is equivalent in behavior and worse in ergonomics, since `Notify` has no "already
  cancelled" state and every waiter must re-check the flag by hand.)
- **The existing `CancelToken` (`Arc<AtomicBool>`, `src/gateway/mod.rs:158`) is retained purely as the
  synchronous mirror**, for the readers that cannot await: the SSE decoder's short-poll check, which
  is what ends a socket that has gone quiet, and the UI thread's own state reads. It is never the
  thing waited on.
- **Update ordering is fixed: set the atomic first, then cancel the token.** A waiter woken by the
  token immediately reads the mirror, so writing the flag first means every observer sees a consistent
  pair; the reverse order allows a woken task to read a mirror that still says "running". The flag is
  written with `Release` and read with `Acquire` so the cancellation is ordered against the work it is
  meant to stop. Nothing ever clears either one — a turn's cancellation is terminal, and a new turn
  gets a fresh child token.
- **The UI performs both writes directly on `0x03`** — no channel hop, so cancellation cannot queue
  behind the backlog of deltas it is trying to stop — and *additionally* sends `TurnControl::Cancel`,
  which is what lets the runtime notice a cancel at a point where it is neither awaiting the token nor
  polling the mirror. The second Ctrl-C exits 130. A cancelled turn still kills the running command's
  **process group**.

**Shutdown: the drain protocol, and why it cannot deadlock.** The naive order — signal, then join —
deadlocks: the worker's turn parks in `send().await` on a full `UiEvent` channel while the UI blocks
in `join`, and neither moves. (Had the producer used `blocking_send` it would not have deadlocked —
it would have panicked inside the runtime, which is worse. See the channel rules above.) The rule that
removes it: **the UI never stops consuming `UiEvent` until the worker has acknowledged the end.**

1. UI cancels — atomic mirror first, then `CancellationToken`, per the ordering above — and sends
   `TurnControl::Shutdown`. It does **not** drop the `UiEvent`
   receiver and does **not** call `join` yet. The receiver staying alive is what guarantees a task
   parked in `send().await` is woken rather than stranded.
2. UI enters a **drain loop**: keep receiving `UiEvent`s and keep committing frames, under a deadline.
   Every receive frees a permit and wakes the parked sender, so the producer always makes progress —
   and because the send is `select!`ed against cancellation, a producer that has already seen the
   cancel unwinds without needing a permit at all.
3. The worker's turn observes cancellation, finishes its `fsync`-and-publish of the session log — a
   torn manifest is worse than a slow exit — and sends exactly one **terminal `UiEvent`**
   (`TurnEnded`/`Error`) as its acknowledgement, then drops its sender and returns. That send is the
   **uncancellable** one: it is awaited plainly, not selected against the token, because cancellation
   is usually the very reason it is being sent. Its liveness comes from step 2 (the UI is draining) and
   its bound from step 5 (deadline ⇒ receiver dropped ⇒ `Err` ⇒ exit), not from the token.
4. The UI leaves the drain loop on that terminal event, or on channel-closed (which the sender's drop
   also produces), and **only then** joins the worker, which is already returning.
5. If the deadline expires first, the UI drops the receiver. A pending `send().await` on a closed
   channel resolves immediately with `Err` — it does not park — so the worker unwinds rather than
   hanging; the UI joins with a short bounded wait and proceeds to restoration regardless of the
   outcome.

The producer's post-terminal path is the one place a drop is allowed, and it is defined: after sending
its terminal event the worker uses `try_send` and **discards on full**, because by then the band is
being torn down and nobody will see those events. The asymmetry is deliberate — *before* the terminal
event nothing may be dropped (a dropped delta is a wrong answer on screen); *after* it, nothing may
block.

Terminal restoration then runs on the UI thread in the upstream order (`app_lifecycle.zig:578-593`):
leave any alternate screen → restore signal dispositions → write the restore sequence → `tcsetattr`
the saved `termios` → move to the footer top and clear downward → exit.

The failure path inverts nothing: if the join times out, restoration still runs, because a terminal
left in raw mode is worse than an unjoined thread in a process that is exiting.

## Signals

An escape-byte `write(2)` from a signal handler restores **screen** state only — cursor visibility,
the alternate screen, the owned band. It does **not** restore the line discipline; that needs
`tcsetattr`. The property that makes this implementable is that POSIX lists `tcsetattr` among the
async-signal-safe functions, so a handler may call it — provided the `termios` it installs was
captured earlier and the handler allocates nothing and takes no lock.

**One owner per signal.**

| Signal | Owner | Handler does |
|---|---|---|
| SIGINT | UI process, async-signal-safe handler | **The handler exists.** `ISIG` only suppresses *terminal-generated* SIGINT; an external `kill -INT`, a `killpg`, or a supervisor still delivers one. It does the same restore pair as TERM/HUP → `SIG_DFL` → re-raise |
| SIGTERM, SIGHUP | UI process, async-signal-safe handler | `write(2)` the compile-time-constant restore bytes → `tcsetattr(TCSAFLUSH)` the `termios` saved at entry → reset the signal to `SIG_DFL` → `raise` it, so the process dies with the right status and the parent sees a signal death rather than a fabricated exit code |
| SIGTSTP | UI process | The restore pair → `SIG_DFL` → `raise(SIGSTOP)`, so the job-control stop is real. **Nothing else** |
| SIGCONT | UI process | **Sets an atomic `resumed` flag, then writes one byte to the non-blocking self-pipe, ignoring `EAGAIN`. That is all.** The work happens on the UI thread (below) |
| SIGWINCH | UI process | Same shape: set an atomic flag, then a best-effort self-pipe byte; debounce and re-layout happen in `collect_facts` (`resize_runtime.zig:46-90`) |
| SIGPIPE | ignored | The UI owns one terminal; a failed write is handled by its return value, not by dying |

**Ctrl-C is unambiguous, and here is why.** In raw mode with `ISIG` cleared the terminal never
generates SIGINT, so byte `0x03` is the *only* way a Ctrl-C reaches xfx and it arrives on the UI
thread's `poll` — cancel the turn, second one exits 130. With the handler installed, any SIGINT that
*does* arrive is by construction external (`kill`, a supervisor, a process-group signal), and the
right response to that is the TERM/HUP response: restore and die by the signal. The two paths cannot
be confused because they cannot both fire for the same event.

**Resume is a two-part mechanism, and the second part is on the UI thread.** A SIGCONT handler must
not re-enter raw mode, re-query layout, or repaint: `tcsetattr` is safe but layout and painting
allocate, take locks, and touch frame state. So:

1. Handler: `resumed.store(true, Release)`; then `write(self_pipe_w, &[1], 1)`. Both are
   async-signal-safe, and the pipe byte is what wakes a `poll` that is otherwise parked for 8 ms.

   The **write end is `O_NONBLOCK`**, and the handler **ignores a short write and `EAGAIN`** — it
   ignores the return value entirely. A full pipe means wakeup bytes are already queued, so the wakeup
   this signal needs is already guaranteed; the *fact* of the signal lives in the atomic, not in the
   byte. Without `O_NONBLOCK` a full pipe would block the handler, which on the UI thread is a hang
   with the terminal raw — the worst failure this whole section exists to avoid. The flag is stored
   **before** the write for the same reason cancellation orders its pair: a UI thread woken by the byte
   must not read a flag that is still false.

   Coalescing rule: write only on a **false→true transition** (`swap(true, AcqRel) == false`). Repeated
   signals of the same kind then cost one byte, the pipe cannot fill under a signal storm, and the
   semantics are unchanged because the UI clears the flag when it acts on it. The read end drains
   greedily — read until `EAGAIN` — and the bytes are discarded unread: they are a wakeup, never data.
   Both ends are `O_NONBLOCK` and both carry `FD_CLOEXEC`, so a spawned command does not inherit them.
2. UI thread, next `collect_facts`: observe the flag and **reinstall the SIGTSTP handler**, then
   re-capture `termios` (the shell may have changed it while stopped), re-enter raw mode, re-query the
   layout, and request a full repaint (`app_lifecycle.zig:609-620,646-656`).

**The reinstall is not optional.** The TSTP handler sets `SIG_DFL` before `raise(SIGSTOP)` so the stop
is genuine — which means that after the first stop the disposition is *default*, and a second
`SIGTSTP` would stop the process **without restoring the terminal**, leaving it raw and unusable.
(Neither stop is delivered by a *typed* Ctrl-Z: raw mode clears `ISIG`, so that keystroke is a byte
for the input decoder and binds nothing. Both are the operator's or a supervisor's signal, and both
land inside `signals::wait_for_input` or not at all — see the raw-mode lifecycle above.) Reinstalling
on resume is what closes that gap, and it belongs on the UI thread because `sigaction` there is
unconstrained. The same reasoning applies to any signal whose handler resets itself to re-raise: TERM,
HUP and INT do not need reinstalling because the process does not survive them.

Also note the wait's interaction with all of this: the UI must watch **stdin and the self-pipe read
end**, not stdin alone as v0.1.0's blocking read does. Without the pipe, a SIGCONT or SIGWINCH that
arrives while the wait is parked is not observed until the next 8 ms tick — usually harmless, but for
resume it means a visible stall on a terminal that is still cooked. `EINTR` is treated as a normal
wakeup, not an error: it is how a delivered signal ends the wait and gives the UI thread its turn.

The call doing that watching is **`pselect(2)`, via `signals::wait_for_input`, and not `poll(2)`** —
earlier drafts of this document said `poll`, and it is unsound here for the reason the stop section
above turns on. A `poll` unmasks and then waits as **two** operations, so a `SIGTSTP` delivered
between them is delivered *outside* the wait, and the handler hands the terminal back cooked while the
session goes on believing it is raw. `pselect` installs the mask, waits, and puts the old mask back,
and the kernel does not let anything in between; the 8 ms tick is expressed as that call's `timeout`
for that reason and no other. Not `ppoll` either: it is Linux-only, macOS has no such call, and
`pselect` is the portable spelling of the same atomicity (Darwin links `pselect$1050`). The invariant
this buys is the one the rest of the section rests on: *while the terminal is raw, `SIGTSTP` is either
blocked or the process is inside `wait_for_input`.*

Three consequences, stated because they correct earlier drafts of this document:

- **`xfx-interrupt` is not "unchanged"; it is not used at all in a TUI session.** It exists because a
  blocked single-threaded runtime cannot observe a signal, and under the TUI signals are observed by
  handlers rather than by a runtime. It stays exactly as it is for the non-TUI commands — `ask` and
  every pipe-friendly path — which is where its rationale still holds. Two mechanisms, disjoint
  sessions, no shared state.
- **The original `termios` is captured once at entry, before raw mode**, into a process-global written
  before any handler is installed and never written again, so a handler can reach it without
  allocating. Everything else a handler touches is a compile-time constant, an atomic, or the
  self-pipe write end.
- **A handler restores and re-raises, or it sets a flag. It never repaints and never returns into the
  UI's frame state.** A cooked terminal with a stale band is a recoverable annoyance; a terminal left
  raw is not.

## Panic ownership

A panic is not a signal, and the restore must be done by **the thread that owns the terminal** — the
UI thread. A global hook that restores unconditionally would let a worker panic mutate terminal state
concurrently with a UI thread that is still painting, which is the same double-writer bug the shadow
grid exists to prevent.

- **Record the UI thread's `ThreadId`** when it takes ownership (immediately after raw mode is
  entered), in the same write-once global that holds the saved `termios`.
- **The global panic hook restores only when `std::thread::current().id()` equals that id.** In that
  case it runs the same restore pair as a signal handler before the default hook prints, so the panic
  message lands on a cooked terminal and is readable rather than painted into a torn band.
- **A worker panic never touches the terminal.** It is caught at the task boundary — `JoinHandle`'s
  `Err(JoinError)`, or an explicit `catch_unwind` around the turn body — and delivered to the UI as a
  **fatal `UiEvent`**. The UI then treats it exactly as a terminal event: leave the drain loop, join,
  restore, report, exit nonzero. The panic message travels as data in that event, so it is printed by
  the UI after restoration instead of racing it.
- **If the UI thread itself panics**, the hook restores and the process aborts the normal way; there
  is nobody left to hand the message to.

This also removes an ordering hazard the earlier draft had: with an unconditional hook, a worker panic
during shutdown could restore the terminal *while* the UI was mid-drain, and the remaining frames
would then be painted onto a cooked terminal.

## The input layer

The boundary upstream draws, and the one this port should keep: **UI owns terminal mechanics (escape
decoding, visual layout, painting); Core owns semantics (editor state, entities, history, slash
routing, approval/question state); the seam is a typed event union** —
`TerminalInputEvent = paste_byte | raw | action` (`input_action.zig:128-132`).

- **Escape decoder subset** (`escape_parser.zig`, a byte-at-a-time stage machine): arrows/Home/End as
  CSI and SS3 (`:456-464,535-553`), modified arrows via `modifiedArrowAction` with the shift bit
  meaning `extend_selection` (`:45-88`), tilde keys `1,3,4,5,6,7,8~` (`:526-533`), bracketed paste
  `200~`/`201~` accepted only with exactly three digits (`:520-524`), bounded unknown-CSI discard
  (max 32 bytes, `:37,234-278`), and the emacs control-byte table (`shortcuts.zig:11-30` — 20 lines,
  port verbatim).
- **Two decoder invariants worth more than the coverage**: *one terminal byte produces at most one
  event* (asserted upstream, `input_action.zig:143-160`), and a bare ESC followed by a control byte
  emits `.escape` **plus** `replay_byte_after_routing` so both route in order
  (`terminal_action_decoder.zig:105-114`). A lone ESC resolves to `.escape` only after a quiet timeout;
  an unknown CSI resolves to `.ignore`, **never a phantom Escape** (test `:342-360`).
- **Editor model** (`editor_state.zig:30-33`): one flat UTF-8 buffer + a byte cursor + an optional
  selection anchor. No line array, no rope. Deletions clamp and remap the anchor (`:190-215`);
  insertion is admission-checked against a byte budget (`:217-225`).
- **Grapheme motion** (`text_boundaries.zig`): a base display unit plus following zero-width
  continuations moves as one — combining accents, flag pairs, skin-tone modifiers, ZWJ families
  (tests `:184-199`). In Rust this is `unicode-segmentation` + `unicode-width`, which is strictly
  better than porting the homegrown walk.
- **Soft wrap** (`visual_layout.zig`) is a pure function of `(text, cursor, cols, entities)` producing
  unit/row events, with two rules that produce fx's feel: **spaces never wrap — they hang past the
  right margin and painters clip them** (`:146-148`), and wrapping is **word-aware** — a word that
  does not fit the remainder but fits a fresh row wraps whole; a word wider than a row splits per
  character (`:278-282`). Vertical motion uses a sticky preferred column
  (`vertical_navigation.zig:32-56`).
- **Growth cap**: the composer may take `content_bottom / 2 + 1` rows
  (`input_presentation.zig:201-205`); byte limits are 8 MiB composer / 4 KiB decision prompts
  (`paste_framing.zig:16-35`).
- **Slash picker**: the trigger is editor-derived, not modal — a leading `/` in the trimmed input arms
  it, and Esc's dismissal is remembered until the trigger *kind* changes
  (`picker_state.zig:83-96,118-136`). Matching ranks exact-command prefix > alias prefix > substring
  (`command_specs.zig:462-499`). The router is a UI-free vtable over `ParsedCommand`
  (`command_router.zig:7-49,51-143`) — in Rust, a trait object or enum dispatch, unit-testable with no
  terminal.
- **Approval panel**: an inline footer panel, 8 rows compact / 11 spacious (spacious at ≥ 34 terminal
  rows, `interaction_state.zig:12-15`); keys 1–3 / ↑↓ / Tab / Enter / Esc / Ctrl-C
  (`ui/input/runtime.zig:65-77`); the wrapped command target is measured and painted through **one**
  shared iterator so the row count cannot drift from the painted rows (`approval_ui.zig:268-294`). The
  three "always" wordings are the contract, and they map onto xfx's existing grant scoping
  (`:1986-1992`): MCP tool → session; `terminal.exec` → "this exact command"; default → "this
  request". Upstream additionally gates the affirmative behind a **readiness commit record** — the
  committed frame at the same request id and dimensions must have actually shown the identity and all
  controls (`approval_readiness.zig:15-39,65-75`); that is the anti-blind-approve property, deferred
  below as hardening.
- **Status/hint row** (`render.zig:391-460`), segments joined by `" · "`, left to right: missing-
  credential call to action, `queued N`, permission mode, **compact model label** (strips `provider/`
  and `claude-` prefixes → `opus 4.7`, `:219-244`), effort label and a fast-mode `⚡︎`, session title,
  `Context: {used}k/{total}k {pct}%`, and workspace identity + `(git branch)` with a budgeted clip.
  The last three are opt-in toggles, default off (`settings_catalog.zig:53-55`). Right-aligned
  overrides in priority order: `esc again to clear` → danger status → upgrade status.
- **Gestures**: Ctrl-C twice within 3000 ms exits, double-Esc within 500 ms clears
  (`gesture_state.zig:3-4`); the transitions are pure functions (`:52-118`). xfx already owns this
  semantics in the line shell — porting it is re-hosting, not inventing.

## Phase 1 and the approval prompt — the decision

Raw mode creates a gap the line shell does not have. `ask` mode requires "a real terminal approval for
every change and every command", and with `ISIG` off and the terminal in raw mode there is no
line-disciplined prompt to fall back to: today's `TtyPrompter` reads a line the kernel assembled. So
Phase 1 must pick one of two, and the honesty contract forbids the third option of quietly behaving
differently.

**Decision: Phase 1 ships the inline approval panel — item 12 moves from Phase 2 into Phase 1.**
The alternative was considered and rejected:

- *Rejected — raw-mode suspend / cooked prompt / restore.* Leave raw mode, print the prompt, read a
  line, re-enter raw mode, repaint. It is small, but it puts a `termios` round trip **on the most
  safety-critical path in the product**, and it inherits every failure mode the restore path has: a
  prompt interrupted mid-suspend leaves the terminal cooked with a half-painted band, and a failure
  to re-enter raw mode after the answer leaves the UI dead while the turn continues. Paying that on
  the approval path, where the user is being asked to authorize a mutation, is the wrong place to be
  clever. It also has to be written and then deleted in Phase 2.
- *Rejected — Phase 1 is auto/yolo-only, `ask` refuses the TUI until Phase 2.* This is honest and
  cheap, and it is the fallback if the panel slips. But it makes the **default-safe mode** the one
  that cannot use the new interface, so every Phase-1 dogfooding session runs in `auto` — the mode
  with no human in the loop — which is exactly backwards for the phase most likely to contain bugs.

What "ships the panel in Phase 1" is allowed to mean, so the scope stays honest: the **inline**
3-choice panel only (1–3 / ↑↓ / Tab / Enter / Esc / Ctrl-C) with the three "always" wordings, painted
in the footer band. The alt-screen file-diff review, the amendment draft, and the readiness commit
gate stay in Phases 2–3 — upstream itself decides inline-vs-screen by diff size (`needsScreen`
`approval_screen.zig:208`), so an inline-only Phase 1 is a narrower version of an existing branch, not
a new behavior.

**If the panel is not ready when Phase 1 otherwise is**, the fallback is the second option and it must
be advertised as such: `ask` under the TUI refuses with a named reason and tells the user to run
`xfx ask` or set `auto`, `docs/parity.md` gains a `partial` row stating the limit, and `--help` does
not offer what the binary will not do. What is forbidden is shipping a TUI where `ask` silently
degrades to `auto`, or where an approval prompt appears but cannot be answered reliably.

## Phase 1 and paste — the decision

**Decision: minimal bracketed-paste framing ships in Phase 1 (item 10). The QA scenario stays in
Phase 1.**

The argument is a correctness hazard, not a feature preference. Phase 1 is the phase in which real
users first drive a raw-mode xfx, and **real users paste** — a stack trace, a diff, a URL, a block of
code. Without `?2004h` and the `200~`/`201~` frame, every byte of that paste is interpreted as a
keystroke: embedded newlines each **submit the composer**, so one paste becomes several prompts sent
to a model and possibly several tool-executing turns; a `\x03` inside pasted text cancels the turn;
any ESC in it is decoded as a key sequence. That is not a cosmetic gap — it is unintended submissions
with real side effects, and it is unrecoverable from the user's side because the damage happens before
they can react.

Framing is also the cheap half. Enabling the mode is one escape sequence xfx already writes as part of
the interactive mode set (`terminal.zig:4`); recognizing the two markers is two states in a decoder
that is being written anyway; and the filter that accepts CR/LF/Tab/printables is a handful of lines
(`paste_framing.zig:112-135`). The 1000-codepoint placeholder is included because a 5000-line paste
rendered literally into the composer would blow past the growth cap and make the band unusable — the
collapse is what keeps paste *safe to display*, and it round-trips verbatim on submit
(`pasted_blocks.zig:53-63`).

**Not in Phase 1, deliberately** (they are Phase 2, item 16): treating the placeholder as an atomic
entity for cursor motion and deletion, entity span shifting on edit, paste-id renumbering during
history recall, and the undo boundary a paste sets. Those make paste *pleasant*; framing makes it
*correct*. A Phase-1 user who backspaces into a placeholder gets a slightly wrong-looking edit; a
Phase-1 user without framing gets four prompts they never sent.

## Acceptance — terminal state, positively proven

Today's guarantee is tested one way: `termios` is byte-identical after exit. Under raw mode that is
necessary and **not sufficient** — a build that failed to enter raw mode at all would pass it. The
acceptance suite is therefore two-sided, and every case below runs on a real pty, in the harness
[`06-qa-harness.md`](06-qa-harness.md) specifies.

**Positive proof of entry** (new, and the one that closes the loophole):

1. Capture `termios` before launch. After the first frame is on screen, read the **child's** terminal
   attributes and assert the raw-mode bits are actually set: `ECHO`, `ICANON`, `IEXTEN`, `ISIG` clear
   in `c_lflag`; `IXON`, `ICRNL`, `BRKINT`, `INPCK`, `ISTRIP` clear in `c_iflag`; `CS8` set;
   `VMIN == 1`, `VTIME == 0` (`shell_runtime.zig:108-138`).
2. Assert the interactive mode sequence was written and that mouse tracking was **not**: `?2004h` and
   `?7l` present, `1000h`/`1002h`/`1006h` absent — upstream pins the same negative
   (`terminal.zig:135-142`), and it is what keeps native scrollback working.
3. Under a simulated tmux environment, assert the kitty push `\x1b[>1u` is **absent**
   (`terminal.zig:29-34`).

**Restoration, one case per exit path** — each asserts `termios` byte-identical to the pre-launch
capture, the cursor visible (`?25h`), and no alternate screen left owned:

Each case asserts exactly what §"Signals" makes the mechanism guarantee — screen bytes *and*
`tcsetattr` from the saved struct — not merely that some bytes were written.

| Case | How it is driven | What must hold |
|---|---|---|
| Normal exit | `/quit` | `termios` byte-identical; transcript still in scrollback; the restore sequence contains **no** `1049l`, because the main surface was never on the alternate screen (`app_lifecycle.zig:39-41`) |
| Panic on the **UI thread** | a build-gated fault injection that panics mid-frame | the hook matched the recorded `ThreadId`, ran the restore pair **before** the default hook printed: `termios` byte-identical and the message readable on a cooked terminal, not painted into a torn band |
| Panic on the **worker** | fault injection inside the turn body | the terminal is **not** touched by the panicking thread; the panic arrives as a fatal `UiEvent`, the UI restores and prints it, exit is nonzero, `termios` byte-identical. A test that asserts only "it exited" would miss the double-writer bug this rules out |
| SIGTERM / SIGHUP | `kill` the child, then `waitpid` | `termios` byte-identical — the handler's `tcsetattr` is what proves this, since escape bytes alone cannot restore a line discipline — **and** the child died *by the signal* (`WIFSIGNALED`, matching signal number), proving the handler re-raised with `SIG_DFL` rather than fabricating an exit code |
| SIGTSTP / SIGCONT | `kill -TSTP`; assert while stopped; then `-CONT` | while stopped: `termios` byte-identical to pre-launch **and** the process is really stopped (`WIFSTOPPED`), which only `raise(SIGSTOP)` after `SIG_DFL` produces. After `-CONT`: re-run the **positive** raw-mode proof and assert a full repaint was committed (`:609-620,646-656`) |
| **Second** SIGTSTP after a resume | `-TSTP`, `-CONT`, `-TSTP` again, assert while stopped | `termios` byte-identical **again** — this is the handler-reinstall gate. Without the reinstall the second TSTP hits the default disposition and stops the process with the terminal still raw, which is precisely the bug this row exists to catch |
| Partial initialization | fail a step — unwritable session store, `poll` that cannot be set up — **after** raw mode is entered but before the first frame | the failure is on stderr **and** `termios` is byte-identical; a half-initialized TUI must not leave a raw terminal. The inverse case (fail *before* raw mode) asserts **no** restore bytes are written at all, since there is nothing to restore |
| External SIGINT | `kill -INT` the TUI child (not a keystroke) | `ISIG` does not protect against this, so the handler must: `termios` byte-identical **and** `WIFSIGNALED` with `SIGINT`. It must **not** be swallowed by an `xfx-interrupt` thread, which does not exist in this session |
| Terminal Ctrl-C | type `0x03` into the pty | **no** signal is delivered (the child does not die); the byte reaches the decoder and cancels the turn, a second exits 130. Together with the row above this proves the two paths are disjoint |

Ctrl-C is tested as a byte, not a signal: with `ISIG` off, `0x03` must reach the decoder and cancel the
turn, and a second one must exit 130 — the same contract the line shell has today, re-proven through
the new input path.

## MVS ladder

Both research notes converge on the same shape: the leap is *owning a bottom band in the normal
buffer and repainting it inside a synchronized-output frame*; cell diffing, fixed-point layout and
commit self-check are hardening on top of that.

**Phase 1 — it is a TUI at all. Shipped.** Every item below is in the binary; its acceptance is the
matrix in `tests/tui.rs` and scenarios 1-12 of [`06-qa-harness.md`](06-qa-harness.md), driven on a
real terminal by `scripts/smoke-tui.sh`.

1. Raw mode + the interactive mode sequence + exact shutdown/signal restore, with restore strings
   ported as constants (`app_lifecycle.zig:36-44`) and the signal contract in §"Signals". Its
   acceptance is the **two-sided** matrix in §"Acceptance — terminal state, positively proven" —
   positive proof that raw mode was entered, plus every restoration case — not the one-sided
   "byte-identical after exit" assert the line shell ships today, which a build that never entered
   raw mode would also pass.
2. Launch cursor probe (CSI `6n`) + push existing shell output into scrollback + a row-1 viewport.
3. The bottom band: `Layout` + footer layout + the 8 ms poll loop + typed render-request reasons.
   **The first commit may repaint the whole owned band inside `?2026h…l`** — synchronized output is
   what removes flicker, and the cell diff is an optimization, not the effect.
4. Transcript store + word wrap + visual-row measurement + **document-append on overflow** (CUP
   bottom, literal `\n`, CRLF normalization) so rows enter native scrollback.
5. Editor loop: the decoder subset above, flat-buffer editor, soft wrap with fx's two rules, sticky
   column, `content_bottom/2 + 1` growth cap.
6. Pacer (adaptive cps + SGR re-open) + Thinking/tool activity row + the 50 ms animation tick.
7. Start-up theme detection (OSC 11 + `COLORFGBG`) and a light/dark palette.
8. Hint row: permission mode + compact model label + context %. Cheap, and it is most of the identity.
9. **The inline approval panel** — 3 choices, `1-3`/↑↓/Tab/Enter/Esc/Ctrl-C, the three "always"
   wordings (`approval_ui.zig:1986-1992`). Promoted from Phase 2 by the decision above: without it
   `ask` cannot run under the TUI, and `ask` is the safe mode.
10. **Bracketed-paste framing, minimal** — enable `?2004h`, recognize `ESC[200~`/`ESC[201~`, route the
    bytes between them as *content* and not as keys, apply the composer filter (CR/LF/Tab/printables),
    and collapse a paste over 1000 codepoints to the placeholder `[Pasted text #N, M lines]` that
    expands verbatim on submit (`paste_framing.zig:112-135`, `pasted_blocks.zig:7,53-63`). See the
    decision below for why this is Phase 1 and what is deliberately *not* in it.

**Phase 2 — release quality. Shipped**, with one named residue in item 17. Items 11-17 are in the
binary and scenarios 13-21 drive them against a release binary on a real terminal.

11. Shadow grid + `diffBand` cell diff + no-op skip, replacing Phase 1's full-band repaint.
12. Resize: SIGWINCH atomic flag + debounce + full repaint (the cursor-probe fingerprint pairing can
    wait).
13. Slash registry + router + the inline picker with dismissal memory. `/model` takes a plain id and
    also browses: a bare one reports the model and the provider at once and the catalog arrives from
    the runtime thread as its own bounded event, because the load is the one network call `/model`
    makes and the thread holding the terminal may not wait on a daemon.
14. Alt-screen file-diff approval and the frame-composed inline restore
    (`app_lifecycle.zig:774-781`, `terminal.zig:22-27`) — the inline panel itself landed in Phase 1.
15. Prompt history (text-only snapshots, draft captured on entry — `composer_history.zig:445-540`),
    which closes the `prompt history` deferred row that today's shell pays for.
16. The **rest** of paste: placeholder atomicity for cursor motion and delete, entity span shifting,
    the paste-id renumbering on history recall, and the undo boundary a paste sets. Phase 1 framed
    the paste and collapsed it; this makes it a first-class entity.
17. OSC 2 title, kitty keyboard with the tmux branch. The title is **borrowed** rather than taken --
    the mode set pushes the terminal's own onto its stack and every restore path pops it back -- and
    the model label is stripped of controls before it goes in, so a configured name cannot close the
    sequence. The kitty push is written on the way in and popped on the way out, and it is **omitted
    entirely under tmux**, where sending it breaks key input; both halves have pty receipts. What is
    *not* here is the **full CSI-u matrix**, which stays deferred with the rest of the breadth below.
    What the binary has is one push flag (`CSI > 1 u`) and its pop (`CSI < u`), and a decoder that
    names the xterm shapes -- the tilde keys and the cursor keys with xterm's single modifier -- and
    answers every `CSI ... u` with a keystroke that binds nothing (`src/tui/input.rs`'s `csi`).
    Owner: the input layer. It is promoted when a binding needs a key those shapes cannot express,
    which is upstream's own reason for the matrix, or when a receipt from a terminal that speaks the
    protocol shows a key this session already claims to support arriving in the `u` form. Either way
    the falsification is one pty case: drive the affected keys and read what the decoder made of
    them, rather than reasoning about what a terminal would send.

**Phase 3 — depth. Not implemented**; every item below is a target and none of it is advertised.

18. Delta undo/redo (100 entries / 1 MB caps, `edit_history.zig:5-6`) + the single-slot kill ring.
19. Question panel with ordinal answers; the freeform "Other" slot after that.
20. Approval **readiness** commit gate and amendment drafts — correctness hardening, not feel.
21. Commit self-check (feed written bytes back into a shadow clone and compare) + partial-write
    recovery + frame retention.
22. Fixed-point layout convergence (phase 1–2 approximate it with one pass: measure footer, then
    transcript).
23. Live theme monitor (mode 2031 / DSR `?996n`) with transcript re-tint and pacer buffer patch.

**Deferred, explicitly** (upstream has them as defense or breadth, and a port earns them later):
full-transcript / subagent-manager / terminal-session alt screens and the owner-handoff transitions;
catalog alt screens; mouse beyond wheel; the full kitty CSI-u matrix; skill `$` tokens and image
tokens/badges; subagent input routing; compact command menus; the model 3-stage picker; queued-prompt
banner cards; tmux `clear-history` and the Apple Terminal RIS path; record tape / ui_observer; WASM.

**"Subagent" means two unrelated things in this epic; neither implies the other.** *fx's in-product
subagents* — the `subagent` tool, the subagent manager alt screen, the subagent panel and its input
routing — are **deferred here and in `docs/parity.md`**, and nothing in this port changes that. The
epic's *"subagent QA"* is the opposite direction: **our own external harness**, Claude agents driving
the built `xfx` binary through a real pty and asserting on what the terminal received. That harness
is not a product surface, ships nothing into the binary, and is specified in
[`06-qa-harness.md`](06-qa-harness.md).

## Crate strategy

- **`rustix` (or `libc`) for termios + `poll(2)` + read/write.** fx's entire terminal layer is
  `tcgetattr/tcsetattr + poll + read + write` (`shell_runtime.zig:103-146,250-278`), and xfx already
  depends on `rustix` for `openat` and process-group kill.
- **Hand-rolled escape sequences and our own shadow grid.** Not a general emulator (vt100 / avt /
  wezterm-term): the contract is "feed the bytes we wrote and get exactly the grid we intended", which
  is why upstream wrote a bounded engine and said so (`engine.zig:1-3`). Required subset:
  CUP/EL/ED/SGR (256 + truecolor)/DECAWM/2026/OSC 8/wide + combining.
- **`crossterm` only where it is honest**: raw-mode RAII, `EnableBracketedPaste`, keyboard-enhancement
  push. **Do not adopt its event parser or `EventStream`** — xfx must own the stdin byte stream to
  interleave cursor-probe responses, theme notifications, and the escape/replay ordering
  (`main.zig:2681-2731`). The note is explicit that taking crossterm's parser costs the byte-level
  `cancel_pending` snapshot and escape-replay ordering; acceptable only if that is a stated v1
  regression, and `vte` or a direct port of the ~450-line pure stage machine is the exit.
- **`unicode-segmentation` + `unicode-width`** for grapheme motion and cell width. Measurement and
  painting must agree cell-for-cell or the diff lies.
- **No async runtime for the UI.** Single-threaded poll loop; agent events arrive on a channel and are
  drained in `collect_facts` (`try_iter`), which is the same shape as upstream's worker tick. xfx's
  existing `tokio` usage stays where the transport is.
- **Syntax highlighting: port fx's four-class lexer** (`code_highlight.zig:8-41`), not `syntect` —
  streaming re-render cost and output stability are the requirement, and matching upstream behavior is
  the goal.
- **`ratatui` is the tempting wrong answer.** A fixed bottom `Layout` chunk is faster to build and
  loses the transcript-scrollback cohabitation model, which is the entire point of §"The one decision
  that matters".

## What breaks next (pre-registered)

- Full-band repaint without a cell diff makes per-frame bytes large over tmux and ssh. No flicker
  (2026 handles that), but bandwidth becomes the complaint — pull phase-2 item 9 forward if it does.
- The pacer's emission and the frame painter touch the same region; skipping the SGR re-open rule
  leaks color mid-stream. Upstream flagged this in a comment rather than in a test
  (`pacer.zig:116-120`).
- Pushing kitty keyboard flags under tmux breaks key input; the branch is mandatory
  (`terminal.zig:29-34`).
- Raw mode inverts today's headline property: "nothing to restore" becomes "a restore path that must
  be right on every exit, including SIGTERM/SIGHUP and SIGTSTP." The abnormal-exit constant write and
  the two-sided pty acceptance suite (§"Acceptance — terminal state, positively proven") are what keep
  the claim true; a one-sided "byte-identical after exit" assertion would pass a build that never
  entered raw mode at all.
