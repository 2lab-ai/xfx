# Changelog

Notable changes to xfx. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) -- with one caveat
worth stating out loud: **a version number here never encodes closeness to
upstream `fx`.** What is and is not implemented lives in
[`docs/parity.md`](docs/parity.md) and nowhere else.

## [0.1.0] - unreleased

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

### Changed

- A bare `xfx` runs the shell instead of exiting 1 with usage. Without a
  terminal it still exits 1, now naming the requirement and pointing at
  `xfx ask`.
