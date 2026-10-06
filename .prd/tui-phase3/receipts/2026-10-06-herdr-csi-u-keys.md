# P3-KEYS Limb B receipt — herdr 0.9.3 (libghostty-vt key encoder), 2026-10-06

Driver: `herdr pane send-keys <pane> <key>` → herdr `handle_pane_send_keys`
(`~/2lab.ai/herdr/src/app/api/panes.rs:975-997`) → `encode_api_keys`
(`src/app/api_helpers.rs:45-57`) → `TerminalRuntime::encode_terminal_key(key, keyboard_protocol())`
(`src/pane.rs:2078-2081`) → libghostty `KeyEncoder`. Same encoder herdr uses for physical keys.

Logger: `keylog.py` (this dir) — raw mode identical to `src/tui/term.rs:114-129` `raw_from`, then writes
`ESC[>4;2m ESC[>1u ESC[?u` (the key part of `MODE_SET`, `term.rs:56`), logs every read as hex.
Logs: `keylog-herdr.jsonl`, `keylog-herdr-2.jsonl` (one key per read, 0.4 s apart, in send order).

Negotiation reply (both runs): `1b5b3f3175` = `ESC[?1u` — the terminal reports kitty flag 1 active.

| key sent | bytes received |
|---|---|
| ctrl+c | `ESC[99;5u` |
| ctrl+d | `ESC[100;5u` |
| ctrl+u | `ESC[117;5u` |
| esc | `ESC[27u` |
| enter | `0d` |
| tab | `09` |
| backspace | `7f` |
| alt+enter | `ESC[13;3u` |
| ctrl+a | `ESC[97;5u` |
| ctrl+e | `ESC[101;5u` |
| ctrl+k | `ESC[107;5u` |
| ctrl+w | `ESC[119;5u` |
| ctrl+y | `ESC[121;5u` |
| ctrl+p | `ESC[112;5u` |
| ctrl+n | `ESC[110;5u` |
| up / down / left / right | `ESC[A` / `ESC[B` / `ESC[D` / `ESC[C` |
| shift+tab | `ESC[9;2u` |
| super+z | `ESC[122;9u` |
| super+shift+z | `ESC[122;10u` |
| ctrl+b | `ESC[98;5u` |
| ctrl+f | `ESC[102;5u` |
| ctrl+h | `ESC[104;5u` |
| ctrl+i | `ESC[105;5u` |
| ctrl+j | `ESC[106;5u` |
| ctrl+l | `ESC[108;5u` |
| ctrl+m | `ESC[109;5u` |
| ctrl+_ | `ESC[95;5u` |
| ctrl+/ | `ESC[47;5u` |
| ctrl+- | `ESC[45;5u` |
| alt+backspace | `ESC[127;3u` |
| alt+left / alt+right | `ESC[1;3D` / `ESC[1;3C` |
| ctrl+left / ctrl+right | `ESC[1;5D` / `ESC[1;5C` |
| shift+enter | `ESC[13;2u` |
| ctrl+enter | `ESC[13;5u` |
| alt+b / alt+f | `ESC[98;3u` / `ESC[102;3u` |
| (home/end/delete) | herdr API rejected these key names (`invalid_key`) — not measured |

Product observation, same pane, release `target/release/xfx` built from the tree committed as `063fc66`:
launched with `env -i … XFX_TUI=1` (fake key, gateway `127.0.0.1:9`), TUI band painted; `ctrl+d` →
no change on screen; `ctrl+c` → no change; `/quit` + enter → `XFX-EXIT=0`. Current decoder pins
`csi(b"99;5", b'u') == Action::Ignore` (`src/tui/input.rs:1146-1152`).

Conclusion: the successor's promotion threshold, limb B ("a receipt from a CSI-u terminal shows a
currently supported key arriving in that form"), is MET — Ctrl-C, Ctrl-D, Ctrl-U, Escape, Alt-Enter and
every bound Ctrl letter arrive in u-form under herdr, and xfx currently drops them.
