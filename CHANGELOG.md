# Changelog

Notable changes to xfx. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) -- with one caveat
worth stating out loud: **a version number here never encodes closeness to
upstream `fx`.** What is and is not implemented lives in
[`docs/parity.md`](docs/parity.md) and nowhere else.

## [Unreleased]

## [0.2.0] - 2026-11-11

The full-screen release. On a terminal, a bare `xfx` now opens a full-screen
interface, and most of the work since 0.1.0 brings that interface, its commands
and its tools to the behaviour of upstream `fx` v0.0.16. That work is recorded
surface by surface in [`docs/parity.md`](docs/parity.md), which says what is
implemented, what is partial and how, and what is absent; this version number
says nothing about how close xfx is to fx. Around it: switching between the AI
Gateway and llmux, MCP servers, skills, image input, saved permission rules, and
a version line that says when a build is from.

If you script against xfx, read **Changed** first. A bare `xfx` on a terminal
now opens the full-screen interface, so a script that drives the line shell
through a pseudoterminal must set `XFX_TUI=0`; and `xfx --version`, the
`status`/`doctor` field names and the `xfx setup` report are different.

### Added

- **The full-screen interface.** It runs on the terminal's normal screen: the
  conversation goes into your own scrollback, and a band at the bottom holds
  the composer, a status row and whichever menu is open. It never turns on
  mouse reporting, so the wheel and native selection keep working, and it
  takes the alternate screen only for the full transcript and for reviewing a
  change too large for the band. The terminal's settings are restored when xfx
  exits, panics or is suspended with Ctrl-Z. If the terminal takes only part of
  a frame, xfx repairs the band or ends the session cleanly and leaves a short
  report in `~/.xfx/last-tui-error.json`.
- **The composer.** Multi-line editing with undo, redo and a kill ring. A paste
  is framed: its line breaks never submit and its control bytes are dropped,
  and a long one shows as `[Pasted text #N, M lines]` while the whole text is
  what is sent. Ctrl-P and Ctrl-N walk the prompt history, kept in
  `~/.xfx/history.jsonl` and switchable in `/settings`. `/` opens command
  completion and `@` a file picker over the workspace. Keys arrive correctly
  from terminals that speak the kitty keyboard protocol, and under tmux.
- **The full transcript.** Ctrl-O opens every entry of the conversation on its
  own screen -- prompts, answers, notices, each tool call with its arguments,
  result and diff, and the time of each entry in your local zone -- and puts
  the conversation back exactly as it was when it closes.
- **Answers and tool calls as they happen.** Answers are rendered as Markdown,
  with highlighted code and tables, in a light or dark palette chosen from
  `XFX_THEME` (`light` or `dark`) or else from the terminal's background, with
  OSC 8 hyperlinks. Tool calls are grouped under their turn with a one-line
  status each. A row above the divider says what a running turn is doing.
- **Approvals and questions in the band.** A tool call that needs permission
  shows what it will do and waits for allow once, always or deny; you can type
  a note with the answer, and the model receives it. A change too large to show
  in the band is reviewed on the full screen. The new `ask_user_question` tool
  lets the model ask 1 to 20 multiple-choice questions, with a free-form answer
  beside the choices, in one panel; outside the full-screen interface it
  reports that it is unavailable. `ask --json` has a new `tool_feedback` event,
  which carries a note given with an approval.
- **Menus and panels.** `/help` is a searchable catalog by category. `/status`
  and a bare `/permissions` open read-only panels. `/model`, `/effort`,
  `/provider`, `/resume`, `/fork`, `/settings`, `/mcp` and `/skills` open
  their menus under the composer.
- **The status row.** It shows the permission mode, the model, the reasoning
  effort when it is not the default, a Fast or Ultra marker when one is on, and
  a missing credential, and `/statusline` adds a context meter, the session and
  the workspace. Each of those three is off until you turn it on. The meter
  appears once a turn has reported its token count: `12k/200k 6%` when the
  catalog gives the model's window, `12k` alone when it does not.
- **Telling the terminal what is happening.** The full-screen interface sends
  OSC 7501 program status reports (`idle`, `working`, `blocked`, `error`) for
  terminals that show them, and rings the terminal bell when a turn ends or
  needs you. `/sound` switches the bell; `XFX_SOUND` sets it for one run; it
  starts on for macOS and off elsewhere.
- **Two providers, switched in place.** `xfx setup gateway` and `xfx setup
  llmux` choose where prompts go, and `/provider` (or `/setup`) does the same
  inside a session. Each provider keeps its own model, so switching back
  restores it. `setup gateway` reads and writes no credential. Setup keeps
  0.1.0's `backend` and `model` keys in step with the new `provider` and
  `models`, so a 0.1.0 binary reading the same profile still reaches the
  endpoint you chose. A provider setting xfx cannot read still refuses every
  turn, and also keeps the catalog, title and `/credits` requests from sending
  the Gateway key.
- **The model menu.** `/model` lists the provider's catalog -- the Gateway's,
  or the one a llmux daemon serves -- with each model's context window, output
  limit and Fast or Ultra availability, ranked by your search; `/model <query>`
  switches to the best match and saves it as that provider's model. The
  catalog is read in the background at launch, again when `/model` asks for
  it, and retried with backoff after a failure; a catalog bigger than 16 MiB or
  slower than 30 seconds is refused whole. `/effort` chooses the reasoning
  effort, and `/fast` and `/ultrafast` the speed, for models whose catalog
  offers them.
- **Usage and credits.** `/usage` is fx's AI Gateway usage dashboard for an
  `AI_GATEWAY_API_KEY`: periods, budget and per-model tables, cached under
  `~/.xfx/usage-cache/`; a fetch starts only while the screen is open. Other
  sign-ins get a short "not shown yet" screen. `/credits` (or `/balance`) shows
  the Gateway account's balance, usage and plan.
- **Sessions you can come back to.** `xfx --resume <last|ID>` opens the
  full-screen interface on a saved session, and `/resume` picks one from this
  workspace or every workspace. `/rename` gives a session your own title, and
  a fresh session can get a generated one. `/fork` starts a new session from
  before one of this session's prompts and leaves the original as it was.
  `/reset` joins `/new` and `/clear`. Leaving a session that had a
  conversation prints the `xfx --resume <id>` command that reopens it.
- **Working with the conversation.** In the full-screen interface, `/compact`
  replaces older turns with a summary to free room, and a turn compacts on its
  own when its next request reaches the compaction threshold or the provider
  refuses one as too large. In both shells, `/undo`
  takes back the newest change this conversation's `write_file` or `edit_file`
  made. `/copy` puts the last reply on the clipboard, and `/trace` writes a
  private diagnostic report about the session and, on macOS, puts the file on
  the clipboard.
- **Image input.** `/image <path>` (or `/img`), `/paste` and Ctrl-V attach a
  PNG, JPEG, GIF or WebP image of up to 20 MiB to the prompt being written,
  after the configured read rules allow the file; `/images` lists or clears
  them. A model without image input is refused rather than sent the image.
- **MCP servers.** The full-screen interface runs an MCP client for stdio
  servers and streamable HTTP servers (with headers and a bearer token named by
  an environment variable; there is no OAuth sign-in). Servers come from
  `~/.xfx/mcp.json` and from a workspace's `.mcp.json`; a workspace's server
  starts only once you approve it in `/mcp`, and the approval is bound to the
  entry it showed you. `/mcp` lists, shows, adds, removes, approves and
  rejects servers, and its menu shows each one's state. A server's tools reach
  the model as `mcp_<server>_<tool>` through two new tools, `capability_search`
  and `mcp_select_tool`, and every call to one goes through the permission
  check. A stdio server starts with six basic environment variables plus the
  ones its entry names, and a workspace's server never gets xfx's own
  credentials.
- **Skills.** A directory holding a `SKILL.md` under `.xfx/skills` or `skills/`
  in the workspace and its parents, or under `~/.xfx/skills`, is offered to the
  model, which loads it with the new `skill` tool -- in `xfx ask` and in both
  shells. `/skills` lists, shows, creates and removes skills, and `add` or
  `install` copies one from a local directory; nothing is fetched from the
  network. Other agents' skill directories are not read.
- **Settings from inside a session.** `/settings` browses and changes the
  interface, agent, notification and advanced settings, such as session
  titles, prompt history, startup scrollback and collapsed tool groups.
  `/workspace` saves extra directories this workspace's turns may use, as
  `ask --add-dir` does for one turn. These commands write
  `~/.xfx/settings.json` and keep every key they did not change, and none of
  these settings can come from a project's `.xfx.json`.
- **Permission rules that are kept.** `/allowlist` manages rules for commands
  and tools in `~/.xfx/settings.json`: a global table, and a table for this
  workspace. A workspace's table, even an empty one, replaces the global table
  in that workspace, its denies included. Changing a global rule while a
  workspace table puts it out of force says so, and so does `/allowlist view`;
  adding a rule to the workspace's own table does not. xfx reads the tables at
  launch. A `~/.xfx/settings.json` that does not parse, or a table in it that
  cannot be read, refuses every tool outside full access, rather than letting
  xfx run without rules that may have been denies. `/permissions` switches
  between `ask`, `auto` and `full access`, and `remember` and `revoke` save or
  withdraw a rule for one exact call in the saved session. A fork keeps every
  deny its source holds.
- **For contributors.** `scripts/check-no-local-paths.sh` refuses a tracked
  file that holds a path belonging to one machine, such as a home directory or
  a temporary folder, and runs in CI.

### Changed

- **A bare `xfx` on a terminal opens the full-screen interface.** In 0.1.0 it
  opened the line-oriented shell, which is still there: `XFX_TUI=0` selects it.
  The line shell now answers every command name the full-screen interface has;
  the 13 that need the full screen (`/resume`, `/fork`, `/settings`, `/mcp`,
  `/usage`, `/stats`, `/compact`, `/effort`, `/fast`, `/ultrafast`, `/image`,
  `/images` and `/paste`) say so and point back to it. Without a terminal on
  both ends, xfx still refuses to open either and points at `xfx ask`.
- **Breaking for scripts: the version line.** `xfx --version` printed the bare
  version; it now prints the version, the channel and the build's id, `xfx 0.2.0
  (preview 2026-11-10-0019-ec40c7ad2e50)`. On a preview, the id is the UTC
  minute of the run that published it and its commit. On any other build, it is
  the UTC minute its commit was made and that commit, read from git. A build
  with neither says its channel alone, `xfx 0.2.0 (debug)`. The shell's
  `/version`, the full-screen welcome line, a new `version` field in `status`
  and a `/trace` report say the same line. `status` still reports
  `build_channel` and `build_revision` on their own.
- **Breaking for scripts: `status` and `doctor` say `provider`.** The fields
  `backend`, `backend_url` and `backend_rejected` are now `provider`,
  `provider_url` and `provider_rejected`, and the `doctor` check `backend` is
  now `provider`. The settings key `backend` still selects the provider, read
  as before, and the new `provider` key selects it too: between two settings
  layers the newer one's key wins, and within one layer `provider` wins over
  `backend`. When the two keys disagree, `doctor` reports it as a `config`
  check named `conflicting_provider_selection`.
- **Breaking for scripts: `xfx setup`'s report says `provider`.** Its
  `backend` line and JSON field are now `provider`; `url` and `models` appear
  only for a provider that has them (`setup llmux` has both, `setup gateway`
  neither); and a new `auth` field names the credential source.
- **`~/.xfx` must be private for every writer.** The settings writers,
  `xfx setup`, `/mcp add` and the skills commands now create a missing
  `~/.xfx` as `0700`. They refuse an existing one that is a symbolic link,
  that is not a directory, or that another account owns, saying which; and
  one open to group or other, naming the `chmod 700 <path>` that makes it
  private. In 0.1.0 only the session store refused such a directory.
- **`HOME` is used only when it is an absolute path.** An empty or relative
  `HOME` now counts as unset, and a `HOME` that names a missing directory is
  refused rather than created. With no usable home, the full-screen interface
  runs without saving anything.
- **"Always" answers can be withdrawn.** As in 0.1.0, an "always" answer is
  recorded with the session and a resume keeps it. `/permissions reset` now
  withdraws a session's grants, and the log records that, so a later resume
  does not bring them back. The full-screen interface records and restores
  grants the same way; in a session that cannot be saved, an "always" lasts
  only as long as the process.
- **Sessions written by 0.2.0 may not open in an older xfx.** 0.2.0 records
  seven event kinds 0.1.0 does not know: `session_renamed`, `images_attached`,
  `tool_images_attached`, `steered`, `tool_feedback`,
  `permission_grants_cleared` and `context_checkpoint`. An older xfx refuses a
  whole session that holds one -- `xfx session` and `ask --resume` say it
  cannot be trusted and exit 1 -- so go back to an older xfx only with
  sessions it wrote.
- **Previews are published per finished body of work, not on every push.** A
  push to `main` publishes a preview only when it changes
  `.github/preview-epoch`, and the key that updates the Homebrew tap can now be
  read only by the publishing job on `main`.

### Fixed

- **`read_file` no longer hangs a turn on a FIFO.** Only directories were
  refused before, so a named pipe with no writer blocked the whole turn. Any
  target that is not a regular file is now refused by name before it is
  opened.
- **A session is much less often reported busy because something forked.**
  xfx forks for every `terminal` command, and a child that had not yet reached
  `exec` briefly kept the session's lock, so a resume landing in that window
  was told another xfx was writing. Taking the lock now retries for about a
  quarter second; a lock that is really held is still refused, a little later.
- **A stopped or timed-out command leaves nothing running.** Stopping signalled
  the command's process group once, and on a loaded macOS machine a child the
  command was still creating could miss the signal and keep running. xfx now
  waits for the command to exit and signals its group until it is empty.
- **The Gateway key no longer follows a redirect or a cleartext proxy.** The
  completion client followed up to ten redirects, replaying the prompt
  wherever a reply pointed and, on the same host, with the key. Every client
  that sends the key now follows none, and uses a proxy from the environment
  only over https.
- **Model text on a terminal has its control characters blanked.** On a
  terminal, `xfx ask` and the line shell wrote the answer's bytes as they came,
  so an escape sequence in it could hide or redraw what came next, including the
  approval question. Control characters other than the line break, carriage
  return and tab are now written as spaces, a pipe or a file still gets the
  answer byte for byte, and `ask --json` on a terminal escapes invisible and
  bidirectional characters. The approval question in `ask` and the line shell
  spells every control character in the command, pattern or path it quotes.
- **`auto` asks before writing files that decide what runs next.** It used to
  admit a write to a shell's startup file, `.ssh`, `.gitconfig`, `.mcp.json`
  and similar files when they were inside the workspace -- for example when
  xfx ran in the home directory. Those writes, and any write inside a git
  directory, now ask, including spellings that a case-insensitive volume
  folds onto those names. `auto`'s read-only `git` commands now also switch
  off filter drivers, hooks and git's trace output, and ask when the
  repository's configuration could still make git run something of its
  choosing.
- **A resumed `ask` uses the session's recorded model only when it is a name
  xfx would write itself**; otherwise it uses the configured model, so an
  edited session log cannot put arbitrary text into the request.
- **Changing model on llmux no longer replays another model's reasoning.** A
  step's signed reasoning is sent back only to the model that produced it; for
  any other, the step is replayed as its text and tool calls.

### Removed

- **`XFX_BUILD_REVISION`**, the build-time variable that named the revision.
  `XFX_BUILD_ID` replaces it; a value not of the form
  `YYYY-MM-DD-HHMM-<12 hex>` fails the build.
- **The `backend`, `backend_url` and `backend_rejected` report fields, and the
  `backend` doctor check**, renamed as described under Changed.
- No command, tool or shell command that 0.1.0 had was removed.

## [0.1.0] - 2026-08-24

The first vertical slice, delivered as one bounded loop rather than a broad
facade: the CLI and configuration core, the Gateway transport and its bounded
SSE decoder, the read tools and multi-step turn, the mutating tools under typed
permission authorities, durable sessions with resume and refreshed project
context, and the interactive shell. Eight tools, six commands, one provider.
Everything else upstream has is absent from the binary and accounted for in
`docs/parity.md`.

There is no earlier release, so everything below is part of this one. The
sections separate what the last slice added from what it repaired in the
slices before it.

### Added

- **The interactive shell.** A bare `xfx` opens a line-oriented append shell on
  the terminal. It refuses to start without a terminal on both stdin and stdout,
  or without a place to record the conversation. It never enters raw mode and
  never takes the alternate screen, so scrollback survives and there is no
  terminal state to restore. Six commands: `/help`, `/new`, `/clear`, `/model`,
  `/version`, `/quit`; anything else beginning with `/` gets one deterministic
  refusal. Each prompt runs one ordinary turn through the same provider, tool
  registry, permission authority, and session store `xfx ask` uses.
- **Tool notices in the shell.** Each tool call is announced on stderr as it
  starts and finishes, with a refusal's reason flattened to one bounded line.
  `xfx ask` is unchanged: its output is its answer.
- **The llmux backend.** Streaming completions from a local
  [llmux](https://github.com/2lab-ai/llmux) daemon over the Anthropic Messages
  wire, chosen by the profile-only `backend` and `llmux_url` settings. It is
  **keyless**: a loopback request carries no `authorization` and no `x-api-key`,
  because that is what the daemon accepts, and xfx never reads or forwards an
  llmux key. That is enforced rather than assumed -- the endpoint must be a
  loopback address literal with an explicit port and no path, under either
  scheme, and neither llmux client honours `HTTP_PROXY` or `ALL_PROXY` -- so
  neither a remote url nor a proxy can carry a keyless prompt off the machine.
  Thinking is not pinned, so responses really can carry reasoning, and every
  content block is preserved verbatim in arrival order -- `thinking` with its
  signature, `redacted_thinking` with its payload, `tool_use` with its parsed
  input, and an unknown type exactly as it arrived -- because Anthropic verifies
  the signature when a tool continuation or a resumed conversation replays the
  assistant's prior reasoning. Reasoning is never rendered: it is preserved for
  the wire and no renderer reads it. The response is a bounded SSE decode --
  bounded per frame, per completion, and by tracked-block count -- that requires
  a `message_delta` stop reason and routes every delta by block index. An
  `error` frame arriving inside an HTTP 200 fails the attempt where it arrives
  and is replayable when its type is transient (`overloaded_error`,
  `rate_limit_error`, `api_error`), so the same upstream condition is worth the
  same number of attempts whether it arrives in band here or as a 429 on the
  Gateway.
- **`xfx setup llmux`**, which points xfx at that daemon. It discovers it --
  an explicit `--url`, else the url a previous setup recorded, else
  `http://127.0.0.1:3456`, else the `proxy.port` in llmux's own config; never a
  scan, never off this machine -- and proves it really is llmux before recording
  anything: `GET /` must answer exactly `llmux` and `GET /models` a non-empty
  catalog, each read through a bounded stream. It then merges `backend`,
  `llmux_url` and a model from that catalog into `~/.xfx/settings.json` through
  a staged `0600` file and a rename, preserving every unrelated key and refusing
  rather than replacing settings it could not parse, and it says on stderr and
  in `overridden_by` when a higher layer will still outrank the file it just
  wrote. It sends **no completion request**, so it costs nothing to run, and it
  reads exactly one field of llmux's own configuration -- `proxy.port` -- and no
  credential.
- **The backend is visible in `status` and `doctor`.** `backend`, and
  `backend_url` when the backend has a configured one, follow `model` directly,
  because a model name means nothing without the endpoint it is asked of; a
  `backend` setting that could not be read is quoted back as `backend_rejected`
  rather than replaced by a default. On llmux, `auth` is
  `llmux-keyless-loopback` rather than a variable name. `doctor` gains a
  `backend` check that appears only when the configured backend cannot run and
  **fails** when it does, because every turn on such a machine refuses; it names
  `xfx setup llmux` or quotes the unreadable value, and it does no network I/O,
  so `doctor` stays a command that is always safe to run.
- **A `sessions` check in `xfx doctor`**, reporting how many sessions are
  recorded, how many session directories could not be trusted, and how many
  staged manifest files an interrupted write left behind. A report, never a
  repair.
- **Two-directional parity reconciliation.** `scripts/check-no-stubs.sh` now
  also fails when an `implemented` row names a surface the binary does not
  advertise, when a name inside a grouped `deferred` row is advertised, and when
  a surface has more than one row. `tests/parity.rs` runs the same
  reconciliation against the running binary: the real parser, the real tool
  schemas, the rendered help pages.
- **`scripts/check-no-secrets.sh`**, a credential scan over the files a push
  would publish.
- **`scripts/smoke.sh`**, an end-to-end check of a built binary: help and
  status, a content-only answer, a five-step read/edit/exec/refuse/finish turn,
  the session lifecycle including resume, rebind and `--no-save`, and the shell
  on a real pseudoterminal. It uses a fake Gateway on a loopback port, never a
  live credential or the network, and leaves raw evidence in a directory outside
  the repository.
- **CI and release workflows** for Linux and macOS on x86_64 and aarch64, each
  on its own native runner. A tag builds, smoke-tests, archives, and checksums
  four target archives.
- **A preview channel.** Every push to `main` publishes a prerelease of that
  exact commit: four native executables and one `SHA256SUMS`, each binary
  stamped `build_channel=preview` with the twelve-character revision it was
  compiled from, and each one gated and smoke-tested on the machine it will run
  on before it is uploaded. The same run renders the Homebrew formula from the
  assets it just published and pushes it to `2lab-ai/tap` over a repository
  scoped deploy key, so `brew install xfx-preview` installs that build rather
  than one from whenever a cron last ran. Because a published release cannot be
  recalled, what can be checked cheaply is checked before it is created -- the
  key is there, it authenticates to GitHub, the tap clones at `master`, and the
  template is in it. Whether the update itself is accepted is decided when the
  push is attempted and is not promised in advance: that push is a hard failure,
  and the tap's scheduled bump remains the recovery path. The prerelease is never
  marked latest, and its version is the timestamp of the run that produced it
  rather than a release number: a preview is a provenance claim, not a version.
  `scripts/check-preview-contract.sh` holds the workflow to all of that,
  including running its no-downgrade comparator rather than grepping for it.
- **Documentation**: `README.md`, `CONTRIBUTING.md`, `docs/architecture.md`, and
  this file.

### Fixed

- **Ctrl-C now reaches the user.** `xfx ask` held locks on stdout and stderr for
  the whole command, so the interrupt watcher -- which runs on another thread and
  exists to say "stopping the turn" -- blocked forever on its first write. The
  first interrupt silently cancelled the turn and the second did nothing at all.
  The streams are no longer locked across a command.
- **Ctrl-C now ends a stalled stream.** Cancellation was only observed when a
  chunk arrived, so interrupting a stream that had started answering and gone
  quiet printed a notice and then waited for a server that had stopped talking.
  The read now re-checks cancellation on a short poll.
- **The interrupt handler is installed before the first prompt.** It is
  registered on the first poll of the signal future, so a Ctrl-C typed in the
  first milliseconds used to meet the default disposition and kill xfx outright.
  Startup now waits, briefly and boundedly, for the handler to exist.

- **The unknown-command refusal cannot be used to paint on the terminal.** A
  shell line beginning `/` is quoted back in the refusal; an escape sequence in
  it was quoted verbatim and obeyed. The quote is now flattened and bounded.

- **Thinking is no longer pinned to `disabled` on llmux.** The pin was there so
  that dropping thinking blocks would be a consequence of the request rather
  than a bet on a server default, but the daemon refuses it for the model
  `xfx setup llmux` selects: `fable` answers HTTP 400, "thinking.type.disabled
  is not supported for this model" (measured on the live wire, 2026-08-22), so
  the pin broke the default configuration outright. The field is omitted, xfx
  takes the adaptive thinking it is given, and preservation rather than
  suppression is the requirement that follows.
- **A tool round survives its own continuation.** A continuation that rebuilt
  the assistant turn from text and tool calls dropped the `thinking` block and
  its signature, and step two of every tool round at the default model was then
  a 400. The decoder now reconstructs every content block in arrival order, in
  the shape the next request has to send back -- accumulating the signature from
  `signature_delta`, keeping a `redacted_thinking` payload that arrives whole at
  block start, and keeping a type this build does not know exactly as it
  arrived. Sessions record them additively, so `ask --resume` puts the same
  blocks back; a record written before this has no field and rebuilds from text
  as before, and a session continued on the Gateway degrades to its
  text-and-tool-call shape instead of failing.
- **An assistant step that reasoned but said nothing visible is still
  recorded.** The terminal path asked one question -- is there text -- where
  there are three, so a completion that reasoned and hit `max_tokens` before
  producing any visible output recorded nothing at all. The session lost a block
  Anthropic requires back unchanged, and with the assistant turn gone the user
  messages on either side of the hole became adjacent, so the rule that merges
  same-role messages folded two separate prompts into a conversation that never
  happened. Both record sites now go through one point that asks all three
  questions. Nothing at all is still not evidence and is still not recorded.
- **A refused `llmux_url` clears rather than defers.** The merge only overwrote
  the key on a value it could accept, so a later layer naming a remote daemon
  was rejected with a diagnostic while the earlier layer's daemon quietly kept
  deciding where the prompt went -- the exact fallback that `backend_rejected`
  exists to refuse, arriving through the other key. A refused value now clears
  the accumulated one, so the operator's most recent word is never silently
  overridden by an older one, and `llmux_url` gained its own layer-source entry
  so `setup` can tell the operator which layer their next turn actually reads.
  Three smaller honesty defects went with it: a client that failed to build on
  the llmux path blamed the Gateway in its error, the integration tests
  constructed providers through the bearer rule rather than the product's own
  loopback gate -- so every loopback property was untested at that level -- and
  the one code path whose job is not to destroy a settings file took a byte
  slice of an identifier where it meant characters.
- **The workflows are linted by a linter that knows their runners.** The pinned
  `actionlint` predated the `macos-15-intel` label and rejected it as unknown,
  failing the preview channel in preflight before anything was built; the pin
  stays, but its version is now part of the contract and checked numerically.

### Changed

- A bare `xfx` runs the shell instead of exiting 1 with usage. Without a
  terminal it still exits 1, now naming the requirement and pointing at
  `xfx ask`.
