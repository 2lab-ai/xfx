# fx trace

Private diagnostic report. It may include prompts, file paths, command output, and file snippets.

## Summary
generated: 2026-10-06T21:35:56Z
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
tokens_total: 0->0
web_search_requests_total: 0 (observed)
billable_web_search_calls: 0 (billed)
auth: source=AI_GATEWAY_API_KEY refreshable=false gateway_team=unset
process: pid=25511 open_fds=12
process_memory:
  PID  PPID    RSS      VSZ ELAPSED STAT
  25511 25473  11216 435360816   00:02 Ss+
worker: processing=false queued=0 cancel=false
project_context: enabled=true not_loaded
FX_TRACE: off
stream: active=false chunks=0 reads=0 lists=0 writes=0 edits=0 commands=0 subagents=0
terminal: 120 cols x 40 rows
metrics: ansi_bytes=2400 full_redraws=0 debounced_resizes=0 footer_updates=0 stream_chunks=0

## Problems
- no obvious errors captured in recent network, tool, compaction, MCP, or model catalog state

## Last Shutdown
(none recorded)

## Context Compaction
(none recorded)

## Session Title
setting: true
title: (none)
generation: status=none

## Network Calls
(none recorded)

## Model Catalog
state=ready entries=1 last_attempt=2s ago
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
cursor: row=3 col=21 viewport_top=1 owned_top=1 last_visible_top=1 last_start_line=0 partial_skip=0
transcript: bytes=123 entries=2 last_rendered_cols=120 dirty=false has_painted=true commit_state=stable committed_start_line=0 pending_compact=false
projection: view=0 history=0 total_rows=3 source_bytes=123 recovery=false catchup=false
replaceable: active=false row=1 start=0
shadow_grid: cols=120 rows=40 cursor=5,3 visible=true sync_active=false
shadow_grid_visible_rows:
  001: 𝒇x v0.0.13 · Run /help for commands
  003: * Preparing trace...
  005: ┃
  007: auto · model-a

## Recent Renderer Events
Internal rendering decisions, not terminal readback. Idle and same-row updates are omitted.
retained=6 capacity=256 overwritten_before=0
[2026-10-06T21:35:54.451Z] seq=1 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=0->1 bytes=0->71 compatible=false geometry=false rebase=true hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..1 footer=3..5
[2026-10-06T21:35:54.452Z] seq=2 kind=commit state=invalid->stable result=committed view=0->0 history=0->0 rows=0->1 layout=0->1ea42a1f1cb31eaa terminal=120x40 cursor=3,3 planned_scroll=0 accepted_scroll=0 bytes=627 repaint=true reset=false recovery=false->false catchup=false->false
[2026-10-06T21:35:56.185Z] seq=3 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=1->1 bytes=71->71 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..1 footer=3..14
[2026-10-06T21:35:56.186Z] seq=4 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=1->1 layout=1ea42a1f1cb31eaa->97c2fbdf88e8b22d terminal=120x40 cursor=3,9 planned_scroll=0 accepted_scroll=0 bytes=1080 repaint=false reset=false recovery=false->false catchup=false->false
[2026-10-06T21:35:56.711Z] seq=5 kind=transition prepared view=0->0 history=0->0 start=0->0 rows=1->3 bytes=71->123 compatible=true geometry=true rebase=false hold=false restore=false catchup=false append=0@1,1 scroll=0 reset=false body=1..3 footer=5..7
[2026-10-06T21:35:56.712Z] seq=6 kind=commit state=stable->stable result=committed view=0->0 history=0->0 rows=1->3 layout=97c2fbdf88e8b22d->57b22e7499c9d9a8 terminal=120x40 cursor=5,3 planned_scroll=0 accepted_scroll=0 bytes=668 repaint=true reset=false recovery=false->false catchup=false->false

## Transcript Timeline
ansi stripped; only obvious secrets masked
[2026-10-06T21:35:54.207Z] raw
  𝒇x v0.0.13 · Run /help for commands

[2026-10-06T21:35:56.711Z] semantic notice
  * Preparing trace...
