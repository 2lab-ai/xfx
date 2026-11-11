# Keys the full transcript leaves unbound, while the @ list is up

These are probes, not parity claims of this unit. The scenarios are not in
`scripts/fxcap/scenarios/`; their lines are in `zz-scratch-at-ctrlo.cap.txt`
and `zz-scratch-at-ctrlo-enter.cap.txt`. All runs are at 40x120 in
`p5-input-at-height-ws`. The builds:
- fx v0.0.16 (`fx/`);
- the base, `v0.1.0-712dd8d` (`xfx-base/`);
- the head, `v0.1.0-3a76208` (`xfx/`).

fx's and the base's `cj-back` came from a longer copy of the first scenario,
whose lines up to `snap cj-back` are the same.

| snap | steps | fx | base | head |
|---|---|---|---|---|
| `cj-back` | `@f-`, Ctrl-O, C-j, Ctrl-O | the composer has a second, empty row; the list is gone | the list is up, the bar moved to f-02 | the list is up, the bar on f-01 |
| `enter-nomatch` | `@zzzz-none`, Ctrl-O, Enter, Ctrl-O | the line is sent (its card, then the gateway's error) | sent | sent |
| `enter-rows` | `@f-`, Ctrl-O, Enter, Ctrl-O | `@f-` is sent | `@items/f-01.txt` completed, nothing sent | `@f-` is sent |

What they show:
- fx's list does not own the surface under the full transcript
  (`filePickerEligible`, `input_completion_runtime.zig:775-786`,
  `:1054-1056@481104fd`).
- Enter: `submitFilePickerOnEnter` answers nothing (`:1034-1037`) and the
  line is sent. The head now does the same (round 3; the base sent only a
  query nothing matched).
- C-j: fx's composer takes it as a new line. The head spends it, as it
  spends Tab and C-k there. That routing is unit CTRLO-AT-KEYS's (since
  copied: `../../ctrlo-at-keys/README.md`).
- The gateway's error is worded differently on fx and xfx, which is not
  this unit's.
