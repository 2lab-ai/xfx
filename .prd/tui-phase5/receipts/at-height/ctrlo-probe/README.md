# Where a key unbound by the full transcript goes while the @ list is up

A probe, not a parity claim of this unit. The scenario is not in
`scripts/fxcap/scenarios/`; its lines are in `zz-scratch-at-ctrlo.cap.txt`
(the copy captured on the head). fx's and the base's captures came from a
longer copy of it whose lines up to `snap cj-back` are the same. All three
are at 40x120, in `p5-input-at-height-ws`. The steps: type `@f-`, then
Ctrl-O, C-j, Ctrl-O.

| snap | fx v0.0.16 (`fx/`) | base `v0.1.0-712dd8d` (`xfx-base/`) | head `v0.1.0-79b8196` (`xfx/`) |
|---|---|---|---|
| `cj-back` | the composer has a second, empty row (`┃`); the list is gone | the list is up, the bar moved to f-02 | the list is up, the bar on f-01 |

What it shows:
- fx's list takes no key under the full transcript. The key goes to the
  composer, and C-j's new line moves the caret off the `@` word
  (`filePickerEligible`, `input_completion_runtime.zig:775-786`,
  `:1054-1056@481104fd`).
- The base's list took the key. The head's takes none (this unit's gate,
  `under_the_full_transcript_the_at_list_takes_no_key`), but does not hand
  it to the composer either.
- The cell comparison (cells.py) differs on both sides: 13 rows on the
  head, 9 on the base. This is outside this unit and has no owner yet.
