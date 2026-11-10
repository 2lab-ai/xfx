---
title: P-panel receipt -- the footer info panel occupant
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, info-panel]
---

# P-panel receipt: the band's read-only info panel

Unit 1 of `research/panels.md` §5 (U-panel): the occupant `/status` and a bare
`/permissions` open in fx v0.0.16, without either command. Spec: fx
`src/core/input/info_panel.zig` and `src/ui/footer/info_panel_presentation.zig`
at `481104fd`. Code: `src/tui/info_panel.rs`, and `src/tui/shell.rs` (the
`Slot::Info` occupant, `offer_to_info_panel`, `open_info_panel`).

## Captures

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  p5-panels-status-short 14x120 <dir>
FX_BIN=<fx v0.0.16>                   scripts/fxcap/capture.sh fx  p5-panels-status-short 24x50  <dir>
FX_BIN=<fx v0.0.16>                   scripts/fxcap/capture.sh fx  p5-panels-keys         40x120 <dir>
XFX_BIN=<this branch's debug build>   scripts/fxcap/capture.sh xfx <the same three>
```

`p5-panels-keys.cap` is the research scenario with one change: each product
gets its own turn and its own `ask`, as `approvals.cap` does (`fx: gateway
p5-panels-keys-turn.json`, `xfx: gateway p5-panels-keys-turn-xfx.json` with
xfx's `terminal` tool, `xfx: env XFX_PERMISSION_MODE=ask`). Without it xfx ran
in `auto` and refused fx's `shell` tool, so no approval could arrive. The fx
side was captured again with the edited file.

The xfx binary is `target/debug/xfx`, built from this branch's working tree
before it was committed, so its banner and `build_revision` say the base,
`e0d97bd5c59c`.

Sanitized: `/private/tmp/claude-<uid>` is written `<tmp>` in every file
(`scripts/fxcap/redact.py`). Nothing else was edited.

## RED, and why no pty pair is GREEN yet

Every pair differs, on the base (`e0d97bd5`) and on this branch alike: the
xfx captures of this branch have the same screen text as the base's (tmp names
normalized). The panel has no opener a user can reach until `/status`
(U-status) and a bare `/permissions` (U-permissions) open it, and both
scenarios open it with `/status`. xfx still writes the `* status:` notice,
and its Shift-Tab still moves the mode, so from `btab` on the keys scenario
runs in `full access` and the approval never arrives.

| pair (fx v0.0.16 / xfx) | rows that differ | waits for |
|---|---|---|
| `status-short-14x120-{open,down1,down-end,up1,ctrl-k}` | 11 each | U-status |
| `status-short-14x120-esc` | 6 | U-status |
| `status-short-24x50-{open,down1,down-end,up1}` / `-ctrl-k` | 19 / 20 | U-status |
| `status-short-24x50-esc` | 12 | U-status |
| `keys-40x120-{btab,tab-pgdn,paste,ctrl-c}` | 18 each | U-status |
| `keys-40x120-after-ctrl-c-esc` | 16 | U-status |
| `keys-40x120-perm-btab` | 25 | U-status and U-permissions (the `* status:` rows are still above it) |
| `keys-40x120-open-before-approval` | 36 | U-status and U-permissions |
| `keys-40x120-{approval-arrives,approval-esc,end}` | 33 each | U-status and U-permissions, then the approval band's command view |

(`cells.py`, row 1 not compared.)

## GREEN: the occupant, against the same captures, in process

What the occupant alone reaches is pinned by tests that fill it with the rows
fx's captures show (a test filler; xfx's own facts are U-status's):

- `src/tui/info_panel.rs`: `next_break` (fit, last space, a leading space,
  the path's last `/`, a column cut, one rune at least, wide runes), the value
  column and its two-column fallback, the header at 1, 2, 3 and 4 rows, the
  row budget (`rows - 10`), the scroll clamp at both ends and at compose time,
  list items, both hint variants and their narrowing, styles, and
  terminal-safe labels and values.
- `src/tui/shell.rs`: the 14x120 and 24x50 screens and their keys; Shift-Tab,
  Tab, Page Up/Down, letters and Ctrl-C swallowed; a paste reaching the hidden
  composer; Enter and Escape closing it; Escape during a turn arming no
  interrupt; an approval in the band, an approval on the review plane, a
  model's question batch and a rule confirmation each taking the keys and the
  screen and giving the panel back as it was; one occupant at a time.

Checked once, not committed: the band rows `Shell::band_rows` paints for 14
snaps, put under fx's own rows 1-2 and filled with each capture's own session
id and path, compared with the fx `.ansi` by `cells.py`: `0 rows differ` for
`status-short-14x120-{open,down1,down-end,up1,ctrl-k}`,
`status-short-24x50-{open,down1,down-end,up1,ctrl-k}` and
`keys-40x120-{btab,tab-pgdn,paste,ctrl-c}`.
