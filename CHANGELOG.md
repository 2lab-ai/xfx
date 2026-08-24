# Changelog

Notable changes to xfx. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) -- with one caveat
worth stating out loud: **a version number here never encodes closeness to
upstream `fx`.** What is and is not implemented lives in
[`docs/parity.md`](docs/parity.md) and nowhere else.

## [Unreleased]

### Fixed

- **`read_file` no longer parks a turn on a FIFO.** Only directories were
  refused before the read, so a named pipe with no writer passed the check and
  blocked the whole turn inside `fs::read`, with no way out. Any target that is
  not a regular file is now refused by name before it can be opened: a socket or
  a device is refused for that same reason, though those usually fail fast
  rather than hang. Upstream `fx` 0.0.5 refuses them all the same way.

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
