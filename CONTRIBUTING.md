# Contributing to xfx

xfx is an unofficial, experimental Rust port of
[`vercel-labs/fx`](https://github.com/vercel-labs/fx). Contributions are
welcome. Before you write code, please read this file and
[`docs/parity.md`](docs/parity.md) -- most of the review friction here comes from
a rule that is unusual, and it is the first one below.

## The rules that are not negotiable

**1. Advertisement is a promise.** A name that appears in `xfx --help`, in the
shell's `/help`, or in a tool schema sent to a model must have a handler, an
acceptance test, and an `implemented` row in `docs/parity.md` -- in the same
change that adds the name. There is no "wire it up later". A surface that is not
finished is absent from the binary, and `scripts/check-no-stubs.sh` plus
`tests/parity.rs` fail the build if it is not.

**2. No stubs.** Production code may not contain `todo!`, `unimplemented!`, a
placeholder success, or canned assistant output. A deferred feature is a row in
the ledger, not a function that returns `Ok(())`.

**3. Deferred means absent.** Adding a deferred surface to the parser or the
registry fails the parity check, in both directions: an `implemented` row for
something the binary does not actually advertise fails too.

**4. Never claim parity.** xfx is a behavioral port of part of a larger product.
Do not write "full parity", "feature complete", or a version number that implies
either.

**5. xfx's own Gateway credential never enters output.** No snapshot, session
event, or log line may carry the token xfx authenticated with. Tests assert this
by planting a key-shaped literal and scanning both streams;
`scripts/check-no-secrets.sh` scans what a push would publish. What the *model*
reads is the opposite promise and must stay documented as one: a `tool_result`
event stores a file's contents or a command's output verbatim, as owner-only
plaintext under `~/.xfx/sessions/<id>/events.jsonl`, and `--no-save` is the only
way to record nothing. Do not write a sentence that denies this; see "Safety, in
plain terms" in [`README.md`](README.md).

## The gate

Everything below must pass before a change is ready. This is exactly what CI
runs, on Linux and macOS, on x86_64 and aarch64:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/check-xfx-identity.sh
./scripts/check-preview-contract.sh
./scripts/smoke.sh target/release/xfx
```

`scripts/check-xfx-identity.sh` scans the tracked tree for the name this port
carried before it was `xfx`. Upstream is `fx` and keeps its own name wherever it
is cited; the retired local name has no valid occurrence left, so the check has
no allowlist -- prose, an identifier, a fake test literal, and a file name are
all findings.

`scripts/check-preview-contract.sh` parses `.github/workflows/preview.yml` and
holds it to what `brew install xfx-preview` needs: the tag and version grammar
the tap parses, the four native rows and their exact asset names, the gate
running before the binary it guards, the `preview` stamp, the five published
files, a prerelease that is not marked latest, and a tap push whose freshness
comparator it extracts and runs rather than reads. It needs `ruby`, which is on
every CI image, and uses `actionlint` when it is installed.

`scripts/smoke.sh` drives the release binary end to end against a fake Gateway
on a loopback port -- no credential, no network -- and prints an evidence
directory containing every captured stream. Quote from it when you report what
you ran.

**Run the binary.** A passing suite is necessary and not sufficient. Before
saying a change is ready, run `./target/release/xfx` and exercise the path you
changed, including the interactive shell if you touched it. Tests do not
construct a terminal for you.

## Tests

- **Write the failing test first**, and say in the pull request what it looked
  like when it was red. A bug fix without a reproduction is a guess.
- **Test behavior, not implementation.** The suite here asserts what the product
  promises: what the binary prints, what it exits with, what it refuses, what it
  sends on the wire, what it leaves on disk.
- **Never fake a terminal.** Shell behavior is tested on a real pseudoterminal
  in `tests/interactive.rs`, because a pipe that claims to be a TTY cannot prove
  echo, canonical mode, signal delivery, or a restored line discipline.
- **Never use a live credential or the network.** `tests/support/fake_gateway.rs`
  binds a loopback port, records exactly what xfx sent, and replays a scripted
  response.
- Name tests as sentences about the product: `a_refused_command_does_not_delete_the_file`
  is a specification; `test_terminal_2` is not.

## Style

- Comments explain **why**, and especially why *not*: what the obvious
  alternative was and what it would have cost. Do not narrate what the next line
  does.
- Cite upstream as `vercel-labs/fx@580a0c5d path/to/file.zig:LINE` when a
  behavior was taken from it. A behavioral claim with no evidence is a guess in
  a nicer font.
- Errors say what happened and what to do about it, and they name the thing they
  are about. Diagnostics go to stderr; a command's answer goes to stdout.
- `rustfmt` decides layout. Where a `#[rustfmt::skip]` exists, it is load-bearing
  and the comment above it says why.

## Changes that need a design decision first

Open an issue before starting on any of these; they change what the product
promises, not just how it keeps its promises:

- widening the automatic command grammar, or any permission default;
- adding a tool, a command, or a slash command;
- changing the session log or manifest format;
- adding a dependency, particularly one that touches TLS, process control, or
  the terminal.

## Pull requests

Include: what changed and why, the red test and the green one, the raw output of
the gate above, and -- if you touched a runtime surface -- the parity rows you
added or changed. Keep the subject a plain imperative sentence.

## License

By contributing you agree that your work is licensed under Apache-2.0, matching
the project and upstream. Do not copy Zig source from upstream into this
repository: xfx is an independent reimplementation, and that is what makes its
attribution honest.
