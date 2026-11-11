# xfx

**An unofficial, experimental Rust port of [`vercel-labs/fx`](https://github.com/vercel-labs/fx).**
Not affiliated with or endorsed by Vercel. Apache-2.0, like the original.

xfx is a small terminal coding agent. You ask it something; it streams an answer
from Vercel AI Gateway or from a local llmux daemon, and it can read your
workspace, change files in it, and
run a bounded set of commands to answer you -- under a permission mode you choose
and a session log you can resume.

It is **a behavioral port of the load-bearing loop, not a reimplementation of the
whole product.** Upstream `fx` is larger: it has subagents, an ACP server,
OAuth logins, web tools and an updater, none of which xfx has. xfx has two
backends behind one provider boundary, twelve tools, seven named commands plus
the shell a bare `xfx` opens, thirty-four slash commands in that shell, and a
full-screen interface -- the default on a terminal -- that is narrower than
upstream's. Everything absent is absent *from the binary*
-- not hidden behind a flag, not stubbed to return success. The row-by-row
account is [`docs/parity.md`](docs/parity.md), and CI fails if the binary ever
advertises something that ledger does not record as implemented.

xfx is not `fx`. The binary is `xfx`, the profile home is `~/.xfx`, and the
project file is `.xfx.json`, so installing it cannot shadow or corrupt an
upstream installation.

## Status

Experimental. Linux and macOS, on x86_64 and aarch64. There is no Windows
build.

Stable builds are cut as `v*` tags, `v0.1.0` being the first. A tag publishes
four native target archives with their checksums and the `2lab-ai/tap/xfx`
Homebrew formula, which selects and installs the archive matching your OS and
architecture.

The **preview channel** runs alongside that. A preview is cut per epic, not per
push: a merge to `main` that bumps `.github/preview-epoch` (one line naming the
epic it closes) publishes a prerelease of that exact commit -- four native
binaries and their checksums -- and each binary reports `build_channel=preview`
along with the twelve characters of the commit it was compiled from, so what you
are running can be tied back to a source revision; a build from a tag answers
`build_channel=release` instead.
[Install](#install) says how to get either in one command.

## What it does

| | |
|---|---|
| **Ask** | `xfx ask "..."` runs one bounded, multi-step turn: streamed assistant text, tool calls executed locally, then exactly one terminal event. `--json` gives you JSONL. |
| **Shell** | `XFX_TUI=0` on a bare `xfx` opens the line-oriented shell instead of the TUI. It never takes the alternate screen, so your scrollback survives. It knows the same thirty-four commands -- `/help` lists them -- and the ones that need the full screen (`/resume`, `/fork`, `/settings`, `/mcp`, `/usage`, `/stats`, `/compact`, `/effort`, `/fast`, `/ultrafast`, `/image`, `/images`, `/paste`) say so and point back to it. |
| **TUI** | A bare `xfx` with a terminal on both ends opens a full-screen band: a divider, the composer and a status row at the bottom of the terminal's **normal** buffer, with a slash-completion menu, an `@` file picker, prompt history, framed paste and all thirty-four commands, their menus and panels among them. It borrows the alternate screen for two things only -- reviewing a change too large for the band, and the full transcript Ctrl-O opens -- and gives it straight back. `XFX_TUI=0` opts out to the line shell; a pipe on either end and every subcommand behave as before. The default, and narrower than upstream's: [`docs/parity.md`](docs/parity.md)'s `full-screen TUI` row is the whole contract. |
| **Models** | `/model` opens a menu of the provider's models -- the Gateway's catalog or a llmux daemon's -- with their context window, output ceiling and Fast/Ultra facts; `/model <query>` switches to the model the query names best (or to the id as typed) from the next turn on, records it in the session and saves it as the provider's model in `~/.xfx/settings.json`. `/setup <gateway\|llmux>` switches which backend a prompt goes to. |
| **Tools** | `list_files`, `glob_files`, `grep_files`, `read_file`, `write_file`, `edit_file`, `create_folder`, `terminal` (one action: `exec`), `capability_search`, `skill`, `mcp_select_tool` and `ask_user_question`. The two MCP tools are offered only where an MCP client runs, the TUI, and `ask_user_question` is answered only there. Reads are bounded; writes are canonicalized inside the workspace, staged in the same directory, and renamed atomically. |
| **Permissions** | `ask` asks you on the terminal, `auto` admits bounded reversible changes and a reporting-only command grammar, `yolo` skips the checks and says so on stderr. |
| **Sessions** | Every turn is recorded to an append-only log under `~/.xfx/sessions/<id>/`, with an atomically published manifest. `xfx sessions`, `xfx session <id>`, and `xfx ask --resume last` read it back. `--no-save` writes nothing at all. |
| **Context** | Bounded `AGENTS.md` instructions from the filesystem root down to your workspace, refreshed every turn rather than remembered. |
| **Diagnostics** | `xfx status [--json]` and `xfx doctor [--json]` report what xfx resolved, without needing a credential and without creating anything. |

## What it does not do

Deliberately, and completely -- these produce an error, never a quiet no-op:
Vercel `login`/`logout`, **Codex and Grok as providers of their own** (no
ChatGPT or xAI subscription OAuth, and no direct route to either; the way to
reach a `gpt-` or `grok-` model here is a llmux daemon that exposes it, see
[Backends](#backends)), ACP, subagents, web tools, background or durable
terminals, the `vision` route for a model without image input, replay, the
`usage` and `credits` subcommands, GitHub workflows, the updater, WASM, and
N-API. `docs/parity.md` records each one with
the upstream evidence for what it is.

Three surfaces exist but are **narrower** than upstream's rather than absent,
and each says where its edge is: the full-screen TUI (the default on a
terminal, and everything it does not do is in the `full-screen TUI` row of the
ledger), the model catalog
(browsed and selected from, with no standalone `models` command), and `xfx
setup` -- which switches providers and proves a llmux daemon, but is **not**
interactive credential onboarding for the Gateway; that part is still absent.
The setup targets are `gateway` and `llmux`; see [Backends](#backends).

Provider *switching* is `xfx setup <provider>` and `/setup <provider>`, which is
where upstream moved it in 0.0.5; the standalone `provider` command name is not
advertised.

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
  families, nothing that compiles or runs your project's code. A directory added
  with `--add-dir` or `/workspace add` is a root like the workspace, and `auto`
  writes there as it does in the workspace -- except the files that decide what
  runs later (`.zshrc`, `.zshenv` and the other shell startup files, all of
  `.ssh`, launch agents and systemd user units, `.git/hooks`, `.git/config`,
  `~/.gitconfig` and every file it includes, `~/.xfx`, a project's `.mcp.json`, the vim, neovim, emacs
  and tmux startup files, `~/.aws/config`, `~/.kube/config`, Claude Code's
  settings), which it leaves to `ask`. That holds in any root,
  in any spelling a case-insensitive volume folds onto those names, and
  through a link: a dotfiles file that `~/.zshenv`, or a link inside `~/.ssh`
  or `~/.vim`, leads to is that file, and when xfx cannot finish checking for
  such links, `auto` asks.
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
- **What you submit at the full-screen shell's composer is kept too.** Every
  prompt and command line -- a collapsed paste as the text it stands for -- is
  appended to `~/.xfx/history.jsonl` (owner-only plaintext) for the next
  session's recall, as upstream keeps `~/.fx/history.jsonl`. `/settings`'
  `Prompt history` off keeps nothing there from then on; lines already kept
  stay until you delete the file.
- **The file tools never write `.git` or `.xfx`.** Refused structurally, before
  any permission check and in every mode including `--yolo`, because a
  `.git/config` entry decides what the commands xfx may then run will execute.

## Install

### Homebrew, from the stable channel

Once a stable release is published, one command adds the `2lab-ai/tap` tap and
installs the newest `v*` release from it:

```bash
brew install 2lab-ai/tap/xfx
```

If the tap is already added, `brew install xfx` and `brew upgrade xfx` are
enough. The formula is named `xfx` and so is the executable it installs, and it
selects and installs the archive matching your OS and architecture.

Both channels install `bin/xfx`, so you have one of them at a time. Run
`brew uninstall xfx-preview` before installing this one, and the reverse before
going back. `xfx status --json` says which one you are holding: a stable build
answers `"build_channel":"release"`, a preview one `"build_channel":"preview"`.

### Homebrew, from the preview channel

```bash
brew install 2lab-ai/tap/xfx-preview
```

That one command adds the `2lab-ai/tap` tap and installs the formula from it. If
the tap is already added, the unqualified name is enough -- and it is what an
upgrade later looks like:

```bash
brew install xfx-preview
brew upgrade xfx-preview
```

The formula is named `xfx-preview`; **the executable it installs is `xfx`.** The
name of the formula is the channel, not the command.

Ask the binary which build it is, and it answers with the version, the channel
and the build's id -- the UTC minute the preview was published, then its commit:

```bash
xfx --version
```

```
xfx 0.2.0 (preview 2026-11-10-0019-52ece6cd8184)
```

`xfx status --json` carries the same line as `version`, and the channel and the
commit on their own as `build_channel` and `build_revision`. A build of your own
names itself the same way, after the minute its commit was made.

macOS and Linux, arm64 and x86_64. Homebrew verifies the SHA-256 of the file it
downloads against the one recorded in the formula, which was taken from the
release the same workflow published.

### By hand, from a preview release

Each preview is a GitHub prerelease tagged
`preview-<date>-<time>-<run>-<attempt>-<sha12>`, carrying four flat executables
-- `xfx-macos-aarch64`, `xfx-macos-x86_64`, `xfx-linux-aarch64`,
`xfx-linux-x86_64` -- and one `SHA256SUMS` covering exactly those four:

```bash
curl -LO https://github.com/2lab-ai/xfx/releases/download/<tag>/xfx-macos-aarch64
curl -LO https://github.com/2lab-ai/xfx/releases/download/<tag>/SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
install -m 0755 xfx-macos-aarch64 ~/.local/bin/xfx
```

### From a tagged release

Each `v*` tag carries one archive per target -- `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin` --
each with its own `.sha256`, and one `SHA256SUMS` covering all four. The example
below is `v0.2.0` for Apple Silicon macOS; substitute the tag you want and the
one of those four targets that matches your machine:

```bash
curl -LO https://github.com/2lab-ai/xfx/releases/download/v0.2.0/xfx-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/2lab-ai/xfx/releases/download/v0.2.0/xfx-aarch64-apple-darwin.tar.gz.sha256
```

The archive is verified before it is installed:

```bash
tar -xzf xfx-<target>.tar.gz
shasum -a 256 -c xfx-<target>.tar.gz.sha256
install -m 0755 xfx-<target>/xfx ~/.local/bin/xfx
```

Every binary on every channel is built and smoke-tested on its own native
runner: nothing is cross-compiled, because the tests that decide whether a build
is publishable open pseudoterminals and run child processes.

## Build

Rust 1.96 or newer:

```bash
git clone https://github.com/2lab-ai/xfx
cd xfx
cargo build --locked --release
./target/release/xfx --version
```

The full local gate, which is what CI runs (CI runs the three script tests,
`test-gate-env.sh`, `test-preview-contract.sh` and `test-check-no-local-paths.sh`,
on one row per OS; see
[`CONTRIBUTING.md`](CONTRIBUTING.md)):

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/check-no-local-paths.sh
./scripts/test-gate-env.sh   # runs ./scripts/test-check-no-secrets.sh too
./scripts/test-check-no-local-paths.sh
./scripts/check-xfx-identity.sh
./scripts/check-preview-contract.sh
./scripts/test-preview-contract.sh
./scripts/smoke.sh target/release/xfx
```

`cargo test` runs the identity gate as one of its tests, and that gate refuses a
tracked file missing from the working tree: a file deleted and not committed,
or renamed with plain `mv`, fails `the_tracked_tree_carries_only_the_current_product_name`
until the change is committed or the file restored.

`scripts/smoke.sh` drives the built binary end to end against a fake Gateway on
a loopback port -- no credential, no network -- and leaves every captured stream
in an evidence directory it prints at the end.

### The parity sweep in CI

[`sweep.yml`](.github/workflows/sweep.yml) runs the fx parity sweep
(`scripts/fxcap/`) on every pull request and every push to `main`, on one
fixed `macos-15` (arm64) runner, against the public fx v0.0.16 release.
[`ci-fetch.sh`](scripts/fxcap/ci-fetch.sh) checks the sha256 of the release
tarball and of the executable in it before anything runs, and fails on a
mismatch. The job needs no secret and gets a read-only token. It runs three
groups of scenarios:

- the scenarios the change adds or edits, including any scenario that uses a
  file the change edits;
- the scenarios the change's receipts name;
- the core set in [`ci-core.txt`](scripts/fxcap/ci-core.txt), which covers
  the composer, a turn, an approval, a menu and a resize.

The job fails when a snap pair that is identical to fx at the merge base is
not identical at the head. Here, identical includes normalized-identical. A
pair that the change adds is reported, not failed. A pair that a
`nondeterminism` cause in the ledger covers is run up to three more times, on
each build, before it counts. [`ci_sweep.py`](scripts/fxcap/ci_sweep.py) is the manual.

When a push bumps `.github/preview-epoch` (an epic's preview), or when someone
starts it by hand, [`sweep-full.yml`](.github/workflows/sweep-full.yml) runs
every scenario and keeps the scoreboard as an artifact. That run reports the
numbers and never fails on them. A green CI run does not replace the local
sweep.

**Why the job measures the base instead of reading a committed floor.** The
job builds xfx twice, at the merge base and at the head, and runs both builds
on the same runner against the same fx. Both runs use the head's scenarios and
ledger, so only the xfx binary differs. The alternative was a committed list
of identical pairs that each pull request updates. It was rejected for three
reasons:

- The list would come from a different machine, at a different time, than the
  check that reads it. Timing on the runner would then show up as false
  regressions, unless every update were regenerated in CI and copied back by
  hand.
- The 0.0.16 delta merges up to seven unit pull requests per epic in
  parallel. Each one would rewrite the same file, so they would conflict.
- A change could lower the list in the same diff that causes the regression.

The cost is a second debug build, about a minute, and a base run of every
scenario that has a pair the head does not pass. The base run confirms that the
base did not pass those pairs either. With the core set as it is today, two
scenarios (`allowlist-remember` and `h1-resize`) have such pairs, so every pull
request pays about a minute for that base run. The cost stays until those pairs
pass.

## Run

On its default backend xfx needs a Vercel AI Gateway credential in the
environment. It reads, in order, a nonblank `VERCEL_OIDC_TOKEN`, then a nonblank
`AI_GATEWAY_API_KEY`:

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

The line shell, which `XFX_TUI=0` opens in place of the full-screen one:

```bash
$ XFX_TUI=0 xfx
xfx 0.2.0 (release 2026-11-10-0019-52ece6cd8184) -- unofficial, experimental Rust port of fx
[shell] model=zai/glm-5.2 permission_mode=auto sandbox=none
[shell] workspace=/home/you/project
[shell] type a prompt, or /help for the 34 commands; Ctrl-D leaves
> what changed in this repo today
```

Ctrl-C stops a running turn; a second one leaves immediately. At the prompt,
Ctrl-C clears the line and twice in a row leaves. `xfx` refuses to open a shell
when stdin or stdout is not a terminal -- use `xfx ask` there.

## Configuration

Later layers override earlier ones, key by key: project `.xfx.json`, then
`~/.xfx/settings.json`, then that file's exact `workspaces["<root>"]` entry, then
the environment (`XFX_MODEL`, `XFX_PERMISSION_MODE`, `XFX_MAX_AGENT_STEPS`).
`XFX_SOUND=off` (or `0`, `false`) starts a session with `/sound` off -- the
switch for the terminal bell the full-screen shell rings when a turn ends and
when it stops to ask you something -- and any other value starts it on;
otherwise the profile's `notifications` decides, and on a machine that never
set it the bell is on for macOS and off elsewhere, as upstream's sounds are.

A link in a workspace skill root (`.xfx/skills`, `skills/`) is followed only
when it resolves inside that workspace, or inside a directory you name: the
profile-only `skill_symlink_authorities` (an array of absolute paths, which a
workspace's own entry replaces) or `XFX_SKILL_SYMLINK_AUTHORITIES`
(colon-separated absolute paths). None is named by default, and an unreadable
list names none at all.

`max_agent_steps` is the one key a project `.xfx.json` can set. Every other key
is **profile-only** -- among them `model`, `models`, `provider`,
`permission_mode`, `backend`, `llmux_url`, the `permission` rules `/allowlist`
writes, and what `/settings`, `/statusline`, `/sound` and `/workspace` save.
Profile-only means a project `.xfx.json` cannot set them and is reported by
`xfx doctor` when it tries: a
repository is shared, so cloning one must not be able to choose the model, the
permission mode, or the endpoint your prompt is sent to.

A settings file that exists but cannot be parsed is reported by `xfx doctor`,
not silently ignored. So is a value that cannot be read: a `backend` xfx does
not recognize does not fall back to a default, because falling back would send
your prompt somewhere you did not choose.

## Backends

Two, chosen by the profile-only `backend` setting.

`gateway` is the default: Vercel AI Gateway over its own wire, authenticated by
the bearer credential above.

`llmux` talks to a [llmux](https://github.com/2lab-ai/llmux) daemon on this
machine over the Anthropic Messages wire. Point xfx at it with:

```bash
xfx setup llmux            # or: xfx setup llmux --url http://127.0.0.1:3456
```

That finds the daemon -- the url a previous setup recorded, else
`http://127.0.0.1:3456`, else the `proxy.port` in llmux's own config -- and
proves it really is llmux before recording anything: `GET /` must answer exactly
`llmux` and `GET /models` must answer a non-empty catalog. It then writes
`backend`, `llmux_url` and a model from that catalog into
`~/.xfx/settings.json`, preserving every other key. It sends **no** completion
request, so it costs nothing to run, and it reads exactly one field of llmux's
own configuration -- `proxy.port` -- and no credential.

**xfx sends nothing to authenticate on this backend.** The daemon accepts a
loopback request without one, and your model credentials stay inside llmux,
which is why `xfx status` reports `auth=llmux-keyless-loopback` rather than a
variable name. That is only
safe while the request does not leave the machine, so it is enforced rather than
assumed: `llmux_url` must be a loopback address with an explicit port and no
path, under either scheme, and a remote host is refused -- including an https
one, because TLS protects a credential and there is no credential here. A remote
llmux would need its own credential story and xfx does not have one. Neither
llmux client honours `HTTP_PROXY` or `ALL_PROXY`, for the same reason.

xfx neither reads nor forwards an llmux key at any point.

**Which models you can reach is the daemon's business, not xfx's.** llmux
publishes a catalog over `GET /models` and xfx browses exactly that: whatever
ids it lists -- including `gpt-` and `grok-` ones, when the daemon is configured
to serve them -- are what `/model` shows and what `/model <id>` will accept. That
is not a Codex or an xAI provider inside xfx: there is no subscription OAuth
here, no second transport, and no credential for either. It is one backend
whose catalog happens to name them, and every request still goes to the loopback
daemon over the Anthropic Messages wire.

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
