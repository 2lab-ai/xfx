# fx trace

Private diagnostic report. It may include prompts, file paths, command output, and file snippets.

## Summary
generated: 2026-10-06T21:36:06Z
version: 0.0.13 (c1db919112c5)
platform: macos/aarch64
build: ReleaseSafe
model: fake/model-a
permission_mode: auto
workspace: <scratch>/work

## Current State
session_id: pmKSE7VJWf63
session_dir: <scratch>/home/.fx/sessions/pmKSE7VJWf63
agent_step_limit: 0
effort: auto
tokens_total: 1200->40
web_search_requests_total: 0 (observed)
billable_web_search_calls: 0 (billed)
auth: source=AI_GATEWAY_API_KEY refreshable=false gateway_team=unset
process: pid=25511 open_fds=17
process_memory:
  PID  PPID    RSS      VSZ ELAPSED STAT
  25511 25473  23904 435430976   00:12 Ss+
worker: processing=false queued=0 cancel=false
project_context: enabled=true bytes=111
FX_TRACE: off
stream: active=false chunks=0 reads=0 lists=0 writes=0 edits=0 commands=0 subagents=0
terminal: 120 cols x 40 rows
metrics: ansi_bytes=7710 full_redraws=0 debounced_resizes=0 footer_updates=0 stream_chunks=0

## Problems
- session title generation failed reason=empty_content

## Last Shutdown
(none recorded)

## Context Compaction
(none recorded)

## Session Title
setting: true
title: say something
generation: status=failed session=pmKSE7VJWf63 model=openai/gpt-5.6-luna reason=empty_content elapsed=11ms

## Network Calls
last=2 ok=2 errors=0 avg=93ms min=11ms max=175ms
session: calls=2 ok=2 errors=0 total_time=0s
coverage: complete (window holds every recorded call)
turns:
  turn 1: calls=1 errors=0 total_time=0s started [2026-10-06T21:36:00.313Z]
[2026-10-06T21:36:00.311Z] model=openai/gpt-5.6-luna source=parent status=200 duration=11ms bytes=0 tokens=1200->40 stop_reason=stop
[2026-10-06T21:36:00.313Z] model=fake/model-a source=parent status=200 duration=175ms bytes=14 tokens=1200->40 stop_reason=stop turn=1 step=1

## Model Catalog
state=ready entries=1 last_attempt=11s ago
selected_model=fake/model-a image_input=non_native vision=false file_input=false tool_use=true
events=1 (always recorded; does not require FX_TRACE)
[2026-10-06T21:35:54.465Z] seq=1 load outcome=ready entries=1 reused_previous=false anonymous_fallback=false fallback_failure=false

## Tool Calls
(none recorded)

## Permissions
permission grants: none

## Runtime Context
TERM: xterm-256color
TERM_PROGRAM: (unset)
LANG: en_US.UTF-8
terminal_hosts: tmux=false cmux=false
mcp_servers: runtime not loaded

## Renderer State
layout: cols=120 rows=40 content_bottom=36 divider_top=37 input_row=38
cursor: row=15 col=21 viewport_top=1 owned_top=1 last_visible_top=1 last_start_line=0 partial_skip=0
transcript: bytes=483 entries=9 last_rendered_cols=120 dirty=false has_painted=true commit_state=stable committed_start_line=0 pending_compact=false
projection: view=0 history=0 total_rows=15 source_bytes=483 recovery=false catchup=false
replaceable: active=false row=1 start=0
shadow_grid: cols=120 rows=40 cursor=17,3 visible=true sync_active=false
shadow_grid_visible_rows:
  001: 𝒇x v0.0.13 · Run /help for commands
  003: * Trace copied to clipboard. Review and redact it before sharing.
  005: ✗ command: Unknown command. Try /help.
  007: ┃ say something
  009:   Traced answer.
  011:   0s (↑4 ↓40)
  013: * Trace copied to clipboard. Review and redact it before sharing.
  015: * Preparing trace...
  017: ┃
  019: auto · model-a

## Recent Renderer Events
Internal rendering decisions, not terminal readback. Idle and same-row updates are omitted.
retained=18 capacity=256 overwritten_before=0
[2026-10-06T21:35:54.451Z] seq=1 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=0->1 bytes=0->71 compatible=false geometry=false rebase=true hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..1 footer=3..5
[2026-10-06T21:35:54.452Z] seq=2 kind=commit state=invalid->stable result=committed view=0->0 history=0->0 rows=0->1 layout=0->1ea42a1f1cb31eaa terminal=120x40 cursor=3,3 planned_scroll=0 accepted_scroll=0 bytes=627 repaint=true reset=false recovery=false->false catchup=false->false
[2026-10-06T21:35:56.185Z] seq=3 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=1->1 bytes=71->71 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..1 footer=3..14
[2026-10-06T21:35:56.186Z] seq=4 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=1->1 layout=1ea42a1f1cb31eaa->97c2fbdf88e8b22d terminal=120x40 cursor=3,9 planned_scroll=0 accepted_scroll=0 bytes=1080 repaint=false reset=false recovery=false->false catchup=false->false
[2026-10-06T21:35:56.711Z] seq=5 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=1->3 bytes=71->123 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..3 footer=5..7
[2026-10-06T21:35:56.712Z] seq=6 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=1->3 layout=97c2fbdf88e8b22d->57b22e7499c9d9a8 terminal=120x40 cursor=5,3 planned_scroll=0 accepted_scroll=0 bytes=668 repaint=true reset=false recovery=false->false catchup=false->false
[2026-10-06T21:35:59.273Z] seq=7 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=3->5 layout=57b22e7499c9d9a8->580b5292091df368 terminal=120x40 cursor=7,3 planned_scroll=0 accepted_scroll=0 bytes=206 repaint=false reset=false recovery=true->true catchup=false->false
[2026-10-06T21:36:00.309Z] seq=8 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=5->5 layout=580b5292091df368->fe8e60b54c6de937 terminal=120x40 cursor=11,3 planned_scroll=0 accepted_scroll=0 bytes=184 repaint=false reset=false recovery=true->true catchup=false->false
[2026-10-06T21:36:00.311Z] seq=9 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=5->7 layout=fe8e60b54c6de937->f5650abc75466f64 terminal=120x40 cursor=11,3 planned_scroll=0 accepted_scroll=0 bytes=0 repaint=false reset=false recovery=true->true catchup=false->false
[2026-10-06T21:36:00.511Z] seq=10 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=7->11 layout=f5650abc75466f64->24b3ff42655796d terminal=120x40 cursor=13,3 planned_scroll=0 accepted_scroll=0 bytes=203 repaint=false reset=false recovery=true->true catchup=false->false
[2026-10-06T21:36:03.331Z] seq=11 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=11->11 bytes=334->334 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..11 footer=13..24
[2026-10-06T21:36:03.332Z] seq=12 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=11->11 layout=24b3ff42655796d->34032eb9ac47841e terminal=120x40 cursor=13,9 planned_scroll=0 accepted_scroll=0 bytes=1085 repaint=false reset=false recovery=true->false catchup=false->false
[2026-10-06T21:36:03.859Z] seq=13 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=11->13 bytes=334->386 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..13 footer=15..17
[2026-10-06T21:36:03.861Z] seq=14 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=11->13 layout=34032eb9ac47841e->ce0c972937774026 terminal=120x40 cursor=15,3 planned_scroll=0 accepted_scroll=0 bytes=947 repaint=true reset=false recovery=false->false catchup=false->false
[2026-10-06T21:36:05.916Z] seq=15 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=13->13 bytes=431->431 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..13 footer=15..26
[2026-10-06T21:36:05.917Z] seq=16 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=13->13 layout=ce0c972937774026->db993bb139261a6a terminal=120x40 cursor=15,9 planned_scroll=0 accepted_scroll=0 bytes=1085 repaint=false reset=false recovery=true->false catchup=false->false
[2026-10-06T21:36:06.437Z] seq=17 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=13->15 bytes=431->483 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..15 footer=17..19
[2026-10-06T21:36:06.438Z] seq=18 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=13->15 layout=db993bb139261a6a->86f0f9ca94c08279 terminal=120x40 cursor=17,3 planned_scroll=0 accepted_scroll=0 bytes=1019 repaint=true reset=false recovery=false->false catchup=false->false

## Transcript Timeline
ansi stripped; only obvious secrets masked
[2026-10-06T21:35:54.207Z] raw
  𝒇x v0.0.13 · Run /help for commands

[2026-10-06T21:35:56.711Z] semantic notice
  * Trace copied to clipboard. Review and redact it before sharing.

[2026-10-06T21:35:59.272Z] semantic notice
  ✗ command: Unknown command. Try /help.

[2026-10-06T21:36:00.309Z] user
  say something

[2026-10-06T21:36:00.499Z] assistant
  Traced answer.

[2026-10-06T21:36:00.489Z] raw
    0s (↑4 ↓40)

[2026-10-06T21:36:03.858Z] semantic notice
  * Trace copied to clipboard. Review and redact it before sharing.

[2026-10-06T21:36:06.437Z] semantic notice
  * Preparing trace...
