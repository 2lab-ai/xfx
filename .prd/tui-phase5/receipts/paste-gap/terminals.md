---
title: PASTE-GAP -- do terminals strip an embedded ESC[201~ from a paste?
type: receipt
tags: [xfx, tui, phase5, paste, security, terminals]
---

# Do terminals strip an embedded `ESC[201~`?

The question: a clipboard holds `text ESC[201~ more`, bracketed paste
(`?2004h`) is on. Does the terminal neutralize the embedded end marker before
it writes the paste to the pty, or pass it through?

Read from each project's source at a pinned commit (no terminal was run with a
real clipboard: captures never touch the clipboard). The survey was done by a
research agent; the three rows marked **re-read** were opened again by this
worker at the same commit and the quoted line found there. tmux's two rows
were also **measured** on this machine's tmux 3.7b (`measure.txt`).

| terminal | verdict | default | citation (`file:line@rev`) | decisive text |
|---|---|---|---|---|
| kitty v0.50.0 | strips (loop until none left; `ESC[` and C1 `0x9b` forms) | always | `kitty/utils.py:1117-1124`, `kitty/window.py:2418-2419` @bb1aca6f | `pat = re.compile(b'(?:(?:\033\\\x5b)\|(?:\x9b))201~')` |
| Alacritty v0.17.0 | strips (every ESC and ETX removed) | always, in bracketed mode | `alacritty/src/event.rs:1383-1386` @94e7c887 | `text.replace(['\x1b', '\x03'], "")` |
| WezTerm 20240203-110809 (and main @458a100c) | strips **once**: a nested marker survives | always | `term/src/terminalstate/mod.rs:833` @5046fc22 (**re-read**) | `canon.replace("\x1b[200~", "").replace("\x1b[201~", "")` |
| iTerm2 v3.7.7 | strips C0 except HT/LF/FF/CR before bracketing; the user can turn it off (Paste Special) | strips | `sources/Pasting/iTermPasteHelper.m:133-139,333-339,434-437,543-553` @e0025340 | `kPasteFlagsRemovingUnsafeControlCodes` |
| xterm #412 | ESC becomes a space (`disallowedPasteControls`); `allowPasteControls` turns it off | strips | `button.c:2665-2671,2834-2878`, `xterm.man:3437-3448` @5c75ead9 (xterm-snapshots mirror) | "the set of control characters which cannot be pasted, converting each into a space" |
| tmux 3.8 / 3.7b, `paste-buffer` | escapes the buffer with `vis(3)` (ESC becomes `^[`) unless `-S` | escapes (since 3.7; 3.6b wrote it raw) | `cmd-paste-buffer.c:46-55`, raw with `-S` at `:108-109,118-119` @7f2a35ad (**re-read**; **measured**) | `utf8_stravisx(&cp, buf, len, VIS_SAFE\|VIS_NOSLASH)` |
| tmux 3.8, a paste **from the outer terminal** | **does not strip, and ends the paste at the first embedded `201~`**: the rest is read as ordinary keys and forwarded to the pane as keys | no option | `tty-keys.c:213-214,641-646` @7f2a35ad (**re-read**; **measured**) | `else if ((*key & KEYC_MASK_KEY) == KEYC_PASTE_END) tty->flags &= ~TTY_BRACKETPASTE;` |
| Terminal.app | unverifiable (closed source, no Apple document found) | unknown | one 2018 user report that it refuses such a paste rather than stripping it (news.ycombinator.com item 16898359), unverified on current macOS | -- |
| GNOME VTE 0.84.1 | C0/C1 replaced with control pictures | always | `src/pastify.cc:57-79` @f2223e45 | -- |
| foot 1.28.0 | ESC dropped from a user paste | always | `selection.c:2242-2251` @ab33c9a1 | -- |
| Windows Terminal v1.26 | C0/C1 removed except HT/LF/CR | always | `ControlCore.cpp:1507-1513` @7ef420cb | -- |

## What follows for xfx

- **No layer can be relied on.** Most terminals strip on a normal paste, but:
  - WezTerm's single-pass replace turns `ESC[20` `ESC[201~` `1~` into a live
    `ESC[201~` (a nested marker; derived from `str::replace` being one
    left-to-right pass);
  - xterm and iTerm2 let the user switch the filter off, and tmux's
    `paste-buffer -S` writes the buffer raw;
  - Terminal.app cannot be checked;
  - **tmux in front of xfx forwards whatever its outer terminal did not strip**,
    and ends the paste at the first embedded marker, so the remainder reaches
    xfx as keystrokes -- as separate key writes, not inside a paste
    (`measure.txt`, mode `outer`).
- So the path "embedded marker + payload reaches xfx's keys" is real: via a
  readiness gap on a direct pty, and via tmux on any outer terminal that does
  not strip.
