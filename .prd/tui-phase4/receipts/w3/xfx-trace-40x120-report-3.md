# xfx trace

Private diagnostic report. It may include prompts, file paths, command output, and file snippets.

## Summary
generated: 2026-10-06T23:28:24Z
version: 0.1.0 (ea9510888cad)
platform: macos/aarch64
build: debug
provider: gateway
model: fake/model-a
effort: auto
permission_mode: auto
workspace: <scratch>/work

## Current State
session_id: 1791329298446-4f7885634798e4a5
session_dir: <scratch>/home/.xfx/sessions/1791329298446-4f7885634798e4a5
session_title: say something
history_turns: 1
tokens_total: 1200->40
agent_step_limit: 25
auth: source=AI_GATEWAY_API_KEY refreshable=false
process: pid=37040
work: outstanding=0 queued=0
terminal: 120 cols x 40 rows
metrics: ansi_bytes=5391, redraws=0, debounced_resizes=0, footer_updates=0, stream_chunks=0

## Configuration
user_settings: <scratch>/home/.xfx/settings.json (absent)
project_settings: <scratch>/work/.xfx.json (absent)
sources: model=process_override permission_mode=compiled_default max_agent_steps=compiled_default provider=compiled_default llmux_url=compiled_default
diagnostics: 0

## Problems
- notice [2026-10-06T23:28:17.423Z] ✗ command: Unknown command. Try /help.

## Network Calls
calls=1 ok=1 refused=0
[2026-10-06T23:28:18.602Z] * network: provider: gateway · model: fake/model-a · 107ms finish: stop tokens: 1200 in · 40 out

## Tool Calls
(none recorded)

## Permissions
permission grants: none

## Session Log
log: <scratch>/home/.xfx/sessions/1791329298446-4f7885634798e4a5/events.jsonl events=6 created=[2026-10-06T23:28:18.447Z] updated=[2026-10-06T23:28:18.634Z]
only obvious secrets masked

turn 1 [2026-10-06T23:28:18.481Z] outcome=final finish_reason=stop steps=1 concluded=[2026-10-06T23:28:18.634Z] output_tokens=40
  user:
    say something
  assistant:
    Traced answer.

## Transcript Timeline
ansi stripped; only obvious secrets masked
[2026-10-06T23:28:14.912Z] semantic notice
  * Trace copied to clipboard. Review and redact it before sharing.

[2026-10-06T23:28:17.423Z] semantic notice
  ✗ command: Unknown command. Try /help.

[2026-10-06T23:28:18.445Z] user
  say something

[2026-10-06T23:28:18.659Z] assistant
  Traced answer.

[2026-10-06T23:28:18.659Z] raw
  0s (↑4 ↓40)

[2026-10-06T23:28:22.015Z] semantic notice
  * Trace copied to clipboard. Review and redact it before sharing.
