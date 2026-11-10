# C3 receipt: /status, /version, /stats

Scenario `scripts/fxcap/scenarios/status-version-stats.cap` (fake gateway turns
in `status-turn.json`), run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  status-version-stats 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx status-version-stats 40x120 <dir>
```

It types `/status`, `/version`, `/stats`, `/stats extra` and `/status now`, each
followed by Enter, then sends one prompt the fake gateway answers with
`MARKER-STATUS-TURN`, then `/status` and `/stats` again. It snaps the screen
after each step. `capture.sh` is unchanged: the same shims (`open`, `pbcopy`,
`pbpaste`, `afplay`, `osascript`), `FX_NO_OPEN_BROWSER=1`, an isolated tmux
socket and an empty `HOME`.

The xfx binary is `target/debug/xfx`, built from the working tree of this
commit. Its banner shows `ea484d5` because that commit is the base.

Sanitized: the capture's scratch directory (`$TMPDIR/fxcap.XXXXXX`, then under
macOS's per-user temporary folder) is written `<scratch>` in every `.txt` and `.ansi`, so the `workspace=` row reads
`<scratch>/work` on both sides. Nothing else was edited.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`).

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `status` | The notice's shape: `* status: model=fake/model-a` on row 3, one `key=value` row per fact under it, two columns in, every row `ESC[38;5;250m…ESC[39m`, a blank row above and below, no echo of `/status`. These rows are byte-identical: the `* status: model=…` head, `auth=AI_GATEWAY_API_KEY`, `auth_refreshable=false`, `permission_mode=auto`, `workspace=<scratch>/work`, `history_turns=0`, `session_permission_grants=0`. | **Rows only one side has.** xfx keeps two facts of its own that `xfx status` already reports: `provider=gateway` (which provider the model is asked of) and `sandbox=none` (xfx does not confine commands). fx's `update_channel=stable` and `ultrafast_requested=false` are about fx's updater and Ultra mode, which xfx does not have, so they are not printed (brief; `general-session.md` §3). **Values that differ.** `build_channel` is `debug` against fx's `stable` (a debug build), and `build_revision` is each binary's own commit. `agent_step_limit` is `25` against fx's `0`, because xfx's default step limit is 25 (`DEFAULT_MAX_AGENT_STEPS`, `src/config.rs:40`). That is the existing `xfx status` fact, which this change does not alter. |
| `version` | Row 16 `* version: <semver>`, with the same SGR (`ESC[38;5;250m* version: …ESC[39m`), no `v` and no build. | Only the number: `0.0.13` against `0.1.0`. C0 already shipped this notice, and this receipt confirms it unchanged. |
| `stats` | Row 18 has the same five keys in the same order, with the same SGR and spacing: `* stats: ansi_bytes=N, redraws=0, debounced_resizes=0, footer_updates=0, stream_chunks=0`. | `ansi_bytes` is `6172` against `4648`. The two painters write different bytes for the same cells. fx also counts its lifecycle writes (mode set and restore, `app_lifecycle.zig:1537`), while xfx counts only what its session loop's writer delivered. Neither number is meant to match the other. |
| `stats-extra` | Row 20 `✗ command: Unknown command. Try /help.` is byte for byte the same (`ESC[38;5;252m✗ command:ESC[38;5;250m Unknown command. Try /help.ESC[39m`). | — |
| `status-now` | Row 22 is the same refusal again. The blank row between the two refusals is shared on both sides. | — |
| `turn` | The card `┃ say the marker`, the answer `  MARKER-STATUS-TURN`, the summary `  0s (↑4 ↓40)` and the band under them are the same on both sides. | The rows above them differ in the same ways as `status`, `version` and `stats` above. |
| `status-after-turn` | `history_turns=1` on both sides, so both report the live conversation and not the store's last session. Every other row is as in `status`. | As `status`. |
| `stats-after-turn` | The `* stats:` row has the same keys, SGR and band under it. On both sides `redraws`, `debounced_resizes`, `footer_updates` and `stream_chunks` are `0`. | `ansi_bytes` is `15894` against `9499`, for the reason given under `stats`. **Rows 28–32 are blank on xfx**, and on fx the notice sits right under the document. This defect was already there before this change and is not part of C3. The `/stats` popup grows the band and carries the document up. When the popup closes, the band shrinks, and the next append lands at the band's old top instead of under the document. The same gap appears on the base binary (`ea484d5`, built without this change) with no C3 command involved: `/help` five times, then `/version` typed through the popup, leaves 2 blank rows before `* version:` on the base binary (3 rows after six `/help`). The gap's size depends on how long the document was. |

## Not in this receipt

- **A non-zero `redraws` or `debounced_resizes`.** The scenario does not resize
  the pane and does not suspend the process. The unit tests in
  `src/tui/event_loop.rs` cover the counters:
  `stats_counts_the_bytes_the_session_delivered_and_none_it_was_refused`,
  `stats_counts_a_whole_repaint_once_when_it_lands` and
  `stats_counts_a_burst_of_winches_as_one_resize_and_a_no_change_as_none`.
  Here fx and xfx part ways on purpose. fx's `full_redraws` is incremented only
  on `.reanchor_top` (`src/ui/resize_runtime.zig:167`), and at `c1db919` only
  `src/ui/resize_tests.zig` passes that mode. So fx always prints `redraws=0`.
  xfx counts the whole repaints that landed after a resume, a `/clear` or a
  Ctrl-L.
- **`/status` during a turn or at an approval.** The runtime thread publishes
  the snapshot whenever a turn, a `/new`, a provider switch or a `/model` ends.
  The band reads that snapshot without waiting on the runtime, so mid-turn
  counts are the ones the previous turn left. The unit tests in
  `src/tui/worker.rs` and `src/tui/shell.rs` cover this, as does the e2e test
  `status_counts_the_turn_the_live_conversation_just_held` in `tests/tui.rs`.
- **A session permission grant.** The scenario approves nothing. The test
  `a_grant_a_model_and_a_new_conversation_each_reach_what_status_reads` in
  `src/tui/worker.rs` covers it.
