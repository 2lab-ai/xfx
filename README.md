# fxr

**An unofficial, experimental Rust port of [`vercel-labs/fx`](https://github.com/vercel-labs/fx).**
Not affiliated with or endorsed by Vercel. Apache-2.0, like the original.

fxr is a small terminal coding agent. You ask it something; it streams an answer
from Vercel AI Gateway, and it can read your workspace, change files in it, and
run a bounded set of commands to answer you -- under a permission mode you choose
and a session log you can resume.

It is **a behavioral port of the load-bearing loop, not a reimplementation of the
whole product.** Upstream `fx` is far larger: it has an interactive TUI, MCP,
skills, subagents, an ACP server, OAuth logins, a model catalog, an updater, and
26 tools. fxr v0.1 has one provider, eight tools, six commands, and a
line-oriented shell. Everything absent is absent *from the binary* -- not hidden
behind a flag, not stubbed to return success. The row-by-row account is
[`docs/parity.md`](docs/parity.md), and CI fails if the binary ever advertises
something that ledger does not record as implemented.

fxr is not `fx`. The binary is `fxr`, the profile home is `~/.fxr`, and the
project file is `.fxr.json`, so installing it cannot shadow or corrupt an
upstream installation.

## Status

Experimental. Version 0.1.0, unreleased. Linux and macOS, on x86_64 and aarch64.
There is no Windows build and no installer; you build it or you download an
archive from a release.

## What it does

| | |
|---|---|
| **Ask** | `fxr ask "..."` runs one bounded, multi-step turn: streamed assistant text, tool calls executed locally, then exactly one terminal event. `--json` gives you JSONL. |
| **Shell** | A bare `fxr` opens a line-oriented shell on your terminal. It never takes the alternate screen, so your scrollback survives. Six commands: `/help`, `/new`, `/clear`, `/model`, `/version`, `/quit`. |
| **Tools** | `list_files`, `glob_files`, `grep_files`, `read_file`, `write_file`, `edit_file`, `create_folder`, and `terminal` (one action: `exec`). Reads are bounded; writes are canonicalized inside the workspace, staged in the same directory, and renamed atomically. |
| **Permissions** | `ask` asks you on the terminal, `auto` admits bounded reversible changes and a reporting-only command grammar, `yolo` skips the checks and says so on stderr. |
| **Sessions** | Every turn is recorded to an append-only log under `~/.fxr/sessions/<id>/`, with an atomically published manifest. `fxr sessions`, `fxr session <id>`, and `fxr ask --resume last` read it back. `--no-save` writes nothing at all. |
| **Context** | Bounded `AGENTS.md` instructions from the filesystem root down to your workspace, refreshed every turn rather than remembered. |
| **Diagnostics** | `fxr status [--json]` and `fxr doctor [--json]` report what fxr resolved, without needing a credential and without creating anything. |

## What it does not do

Deliberately, and completely -- these produce an error, never a quiet no-op:
Vercel `login`/`logout`/`setup`, Codex OAuth, a model catalog or provider
switching, ACP, MCP, skills, subagents, web tools, background or durable
terminals, images and vision, a full-screen TUI, replay, usage and credits,
GitHub workflows, the updater, WASM, and N-API. `docs/parity.md` records each
one with the upstream evidence for what it is.

**fxr does not claim parity with `fx`, and it never will claim it in a version
number.** If a claim in this file disagrees with `docs/parity.md`, the ledger is
right; if the ledger disagrees with the code, the code is right and both
documents are bugs.

## Safety, in plain terms

Read this before using `--auto` or `--yolo` on a repository you care about.

- **There is no OS sandbox.** fxr reports `sandbox=none` in `status` because
  that is true. A command fxr agrees to run runs with your privileges, with your
  environment, with your network. The permission modes decide *what fxr agrees
  to start*; they cannot constrain what a started process then does.
- **`ask` (the safest mode)** requires a real terminal approval for every change
  and every command. With no terminal -- in a pipe, in CI -- it refuses instead of
  asking, so a scripted `fxr ask` fails closed.
- **`auto` (the default)** runs reads directly, runs bounded reversible
  workspace writes after structural validation, and admits only a narrow
  reporting command grammar: no `&&`, no shell, no package-manager build or test
  families, nothing that compiles or runs your project's code. It will not write
  into a directory added with `--add-dir`.
- **`yolo` runs no permission check at all.** It prints a warning to stderr
  every time. Use it in a container you are willing to lose.
- **Approvals are scoped.** Answering "always" grants exactly one tool and one
  target, and it is recorded against one session id -- `fxr session <id>` lists
  every standing grant by name.
- **Credentials are never persisted.** No session event, snapshot, or tool
  result can carry one; fxr reads a token from the environment and sends it to
  one endpoint.

## Install

There is no package yet. Either build from source, or download the archive for
your platform from a release and verify it:

```bash
tar -xzf fxr-<target>.tar.gz
shasum -a 256 -c fxr-<target>.tar.gz.sha256
install -m 0755 fxr-<target>/fxr ~/.local/bin/fxr
```

Targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, and `aarch64-apple-darwin`. Each archive is built and
smoke-tested on its own native runner.

## Build

Rust 1.96 or newer:

```bash
git clone https://github.com/2lab-ai/fxr
cd fxr
cargo build --locked --release
./target/release/fxr --version
```

The full local gate, which is what CI runs:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/smoke.sh target/release/fxr
```

`scripts/smoke.sh` drives the built binary end to end against a fake Gateway on
a loopback port -- no credential, no network -- and leaves every captured stream
in an evidence directory it prints at the end.

## Run

fxr needs a Vercel AI Gateway credential in the environment. It reads, in order,
a nonblank `VERCEL_OIDC_TOKEN`, then a nonblank `AI_GATEWAY_API_KEY`:

```bash
export AI_GATEWAY_API_KEY=...
```

One question:

```bash
fxr ask "what does src/agent/machine.rs do"
```

Let it change things, and watch what it decides:

```bash
fxr ask --auto --json "add a doc comment to the run_turn function"
```

Continue where you left off:

```bash
fxr sessions
fxr ask --resume last "now do the same for run_turn_saved"
```

The shell:

```bash
$ fxr
fxr 0.1.0 (release, revision 52ece6cd8184) -- unofficial, experimental Rust port of fx
[shell] model=zai/glm-5.2 permission_mode=auto sandbox=none
[shell] workspace=/home/you/project
[shell] type a prompt, or /help for the 6 commands; Ctrl-D leaves
> what changed in this repo today
```

Ctrl-C stops a running turn; a second one leaves immediately. At the prompt,
Ctrl-C clears the line and twice in a row leaves. `fxr` refuses to open a shell
when stdin or stdout is not a terminal -- use `fxr ask` there.

## Configuration

Later layers override earlier ones, key by key: project `.fxr.json`, then
`~/.fxr/settings.json`, then that file's exact `workspaces["<root>"]` entry, then
the environment (`FXR_MODEL`, `FXR_PERMISSION_MODE`, `FXR_MAX_AGENT_STEPS`).
Three keys are read -- `model`, `permission_mode`, `max_agent_steps` -- because
those are the three the runtime consumes. A settings file that exists but cannot
be parsed is reported by `fxr doctor`, not silently ignored.

## How it works

[`docs/architecture.md`](docs/architecture.md) has the real data flow, module by
module, with the boundaries that make it testable without a credential. The
short version: `cli` decides what was asked, `config` resolves how, `agent`
drives one bounded turn against a `Provider`, `tools` execute under a one-use
authority minted by `permission`, `session` records what happened, and `output`
is the only thing that writes bytes.

## Upstream and attribution

fxr is an independent reimplementation pinned to `vercel-labs/fx` at
`580a0c5da9386317251968c09c1cee69e763487a`. No Zig source is copied. Every
behavioral claim in the ledger cites a file and line at that commit.

- Upstream: `vercel-labs/fx`, Apache-2.0, Copyright 2025 Vercel, Inc.
- fxr: Apache-2.0, Copyright 2026 2lab.ai.

See [`UPSTREAM.md`](UPSTREAM.md) for the attribution, what behavior was taken
from where, and every deliberate deviation. `LICENSE` and `NOTICE` travel with
every archive.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md). The one rule worth stating here:
**advertisement is a promise.** A name in `--help`, in `/help`, or in a tool
schema must have a handler, a test, and an `implemented` row in the ledger, in
the same change.
