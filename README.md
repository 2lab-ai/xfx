# xfx

**An unofficial, experimental Rust port of [`vercel-labs/fx`](https://github.com/vercel-labs/fx).**
Not affiliated with or endorsed by Vercel. Apache-2.0, like the original.

xfx is a small terminal coding agent. You ask it something; it streams an answer
from Vercel AI Gateway, and it can read your workspace, change files in it, and
run a bounded set of commands to answer you -- under a permission mode you choose
and a session log you can resume.

It is **a behavioral port of the load-bearing loop, not a reimplementation of the
whole product.** Upstream `fx` is far larger: it has an interactive TUI, MCP,
skills, subagents, an ACP server, OAuth logins, a model catalog, an updater, and
26 tools. xfx v0.1 has one provider, eight tools, six commands, and a
line-oriented shell. Everything absent is absent *from the binary* -- not hidden
behind a flag, not stubbed to return success. The row-by-row account is
[`docs/parity.md`](docs/parity.md), and CI fails if the binary ever advertises
something that ledger does not record as implemented.

xfx is not `fx`. The binary is `xfx`, the profile home is `~/.xfx`, and the
project file is `.xfx.json`, so installing it cannot shadow or corrupt an
upstream installation.

## Status

Experimental. Version 0.1.0, unreleased: nothing has been published yet, so
today you build it. Linux and macOS, on x86_64 and aarch64. There is no Windows
build and no installer.

A public `preview` channel -- prerelease builds, each reporting
`build_channel=preview` and the exact source revision it was compiled from -- is
planned. The binary already carries the field that will name it; there is
nothing to install from it yet, and this page will say how when there is.

## What it does

| | |
|---|---|
| **Ask** | `xfx ask "..."` runs one bounded, multi-step turn: streamed assistant text, tool calls executed locally, then exactly one terminal event. `--json` gives you JSONL. |
| **Shell** | A bare `xfx` opens a line-oriented shell on your terminal. It never takes the alternate screen, so your scrollback survives. Six commands: `/help`, `/new`, `/clear`, `/model`, `/version`, `/quit`. |
| **Tools** | `list_files`, `glob_files`, `grep_files`, `read_file`, `write_file`, `edit_file`, `create_folder`, and `terminal` (one action: `exec`). Reads are bounded; writes are canonicalized inside the workspace, staged in the same directory, and renamed atomically. |
| **Permissions** | `ask` asks you on the terminal, `auto` admits bounded reversible changes and a reporting-only command grammar, `yolo` skips the checks and says so on stderr. |
| **Sessions** | Every turn is recorded to an append-only log under `~/.xfx/sessions/<id>/`, with an atomically published manifest. `xfx sessions`, `xfx session <id>`, and `xfx ask --resume last` read it back. `--no-save` writes nothing at all. |
| **Context** | Bounded `AGENTS.md` instructions from the filesystem root down to your workspace, refreshed every turn rather than remembered. |
| **Diagnostics** | `xfx status [--json]` and `xfx doctor [--json]` report what xfx resolved, without needing a credential and without creating anything. |

## What it does not do

Deliberately, and completely -- these produce an error, never a quiet no-op:
Vercel `login`/`logout`/`setup`, Codex OAuth, a model catalog or provider
switching, ACP, MCP, skills, subagents, web tools, background or durable
terminals, images and vision, a full-screen TUI, replay, usage and credits,
GitHub workflows, the updater, WASM, and N-API. `docs/parity.md` records each
one with the upstream evidence for what it is.

**xfx does not claim parity with `fx`, and it never will claim it in a version
number.** If a claim in this file disagrees with `docs/parity.md`, the ledger is
right; if the ledger disagrees with the code, the code is right and both
documents are bugs.

## Safety, in plain terms

Read this before using `--auto` or `--yolo` on a repository you care about.

- **There is no OS sandbox.** xfx reports `sandbox=none` in `status` because
  that is true. A command xfx agrees to run runs with your privileges, with your
  environment, with your network. The permission modes decide *what xfx agrees
  to start*; they cannot constrain what a started process then does.
- **`ask` (the safest mode)** requires a real terminal approval for every change
  and every command. With no terminal -- in a pipe, in CI -- it refuses instead of
  asking, so a scripted `xfx ask` fails closed.
- **`auto` (the default)** runs reads directly, runs bounded reversible
  workspace writes after structural validation, and admits only a narrow
  reporting command grammar: no `&&`, no shell, no package-manager build or test
  families, nothing that compiles or runs your project's code. It will not write
  into a directory added with `--add-dir`.
- **`yolo` runs no permission check at all.** It prints a warning to stderr
  every time. Use it in a container you are willing to lose.
- **Approvals are scoped.** Answering "always" grants exactly one tool and one
  target -- the target being the file's absolute path, so an approval cannot
  follow a resumed session into another workspace -- and it is recorded against
  one session id; `xfx session <id>` lists every standing grant by name.
- **xfx's own Gateway credential is never persisted.** No session event,
  snapshot, or tool result carries it: xfx reads the token from the environment
  and sends it to one endpoint. **What the model reads is a different question,
  and the answer is that it is saved.** Every file's contents and every
  command's output that a tool returns is written verbatim to
  `~/.xfx/sessions/<id>/events.jsonl` as owner-only (`0600`) plaintext, so if
  you have the model read a file holding your own secrets, that secret is on
  disk in the session log. Use `--no-save` for a turn that must leave nothing
  behind.
- **The file tools never write `.git` or `.xfx`.** Refused structurally, before
  any permission check and in every mode including `--yolo`, because a
  `.git/config` entry decides what the commands xfx may then run will execute.

## Install

There is no package and no published release yet, so the way to get it is to
build it -- see [Build](#build). When a tagged release exists it carries one
archive per target, and the archive is verified before it is installed:

```bash
tar -xzf xfx-<target>.tar.gz
shasum -a 256 -c xfx-<target>.tar.gz.sha256
install -m 0755 xfx-<target>/xfx ~/.local/bin/xfx
```

Targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, and `aarch64-apple-darwin`. Each archive is built and
smoke-tested on its own native runner.

The planned `preview` channel is not one of these commands yet. Until it
publishes something, no install line for it is written down here: an instruction
that does not work is worse than a missing one.

## Build

Rust 1.96 or newer:

```bash
git clone https://github.com/2lab-ai/xfx
cd xfx
cargo build --locked --release
./target/release/xfx --version
```

The full local gate, which is what CI runs:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/check-xfx-identity.sh
./scripts/smoke.sh target/release/xfx
```

`scripts/smoke.sh` drives the built binary end to end against a fake Gateway on
a loopback port -- no credential, no network -- and leaves every captured stream
in an evidence directory it prints at the end.

## Run

xfx needs a Vercel AI Gateway credential in the environment. It reads, in order,
a nonblank `VERCEL_OIDC_TOKEN`, then a nonblank `AI_GATEWAY_API_KEY`:

```bash
export AI_GATEWAY_API_KEY=...
```

One question:

```bash
xfx ask "what does src/agent/machine.rs do"
```

Let it change things, and watch what it decides:

```bash
xfx ask --auto --json "add a doc comment to the run_turn function"
```

Continue where you left off:

```bash
xfx sessions
xfx ask --resume last "now do the same for run_turn_saved"
```

The shell:

```bash
$ xfx
xfx 0.1.0 (release, revision 52ece6cd8184) -- unofficial, experimental Rust port of fx
[shell] model=zai/glm-5.2 permission_mode=auto sandbox=none
[shell] workspace=/home/you/project
[shell] type a prompt, or /help for the 6 commands; Ctrl-D leaves
> what changed in this repo today
```

Ctrl-C stops a running turn; a second one leaves immediately. At the prompt,
Ctrl-C clears the line and twice in a row leaves. `xfx` refuses to open a shell
when stdin or stdout is not a terminal -- use `xfx ask` there.

## Configuration

Later layers override earlier ones, key by key: project `.xfx.json`, then
`~/.xfx/settings.json`, then that file's exact `workspaces["<root>"]` entry, then
the environment (`XFX_MODEL`, `XFX_PERMISSION_MODE`, `XFX_MAX_AGENT_STEPS`).
Three keys are read -- `model`, `permission_mode`, `max_agent_steps` -- because
those are the three the runtime consumes. A settings file that exists but cannot
be parsed is reported by `xfx doctor`, not silently ignored.

## How it works

[`docs/architecture.md`](docs/architecture.md) has the real data flow, module by
module, with the boundaries that make it testable without a credential. The
short version: `cli` decides what was asked, `config` resolves how, `agent`
drives one bounded turn against a `Provider`, `tools` execute under a one-use
authority minted by `permission`, `session` records what happened, and `output`
is the only thing that writes bytes.

## Upstream and attribution

xfx is an independent reimplementation pinned to `vercel-labs/fx` at
`580a0c5da9386317251968c09c1cee69e763487a`. No Zig source is copied. Every
behavioral claim in the ledger cites a file and line at that commit.

- Upstream: `vercel-labs/fx`, Apache-2.0, Copyright 2025 Vercel, Inc.
- xfx: Apache-2.0, Copyright 2026 2lab.ai.

See [`UPSTREAM.md`](UPSTREAM.md) for the attribution, what behavior was taken
from where, and every deliberate deviation. `LICENSE` and `NOTICE` travel with
every archive.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md). The one rule worth stating here:
**advertisement is a promise.** A name in `--help`, in `/help`, or in a tool
schema must have a handler, a test, and an `implemented` row in the ledger, in
the same change.
