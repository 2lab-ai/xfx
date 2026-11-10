#!/usr/bin/env python3
"""Train 14 (new): merge 1's four conflict blocks in `src/tui/event_loop.rs`,
resolved by hand and written down here, so `reproduce.sh` can redo them. The
file is read in git's diff3 style (ours, `|||||||` base, `=======` theirs).
Each block is first checked to be the one read by hand -- the sha256 (16 hex
digits) of its ours, base and theirs lines -- and replaced by the text below;
any other block, or a fifth, stops the script, changing nothing.

Ours is main (`dcfa5146`): `commit_frame_led` borrows the tick's
`Reconciled` (`&reconciled`), `commit_tick` writes a submitted card's frame
after its preview's frame (`take_preview_shown`), and the `/model` column's
ask reads the catalog's clock (`catalog_now`). Theirs is the `@` stack
(`4130f4ac`, AT-FOLDERS' hold): `commit_frame_led` takes `held`, the tick's
frame waits for a folder listing's rows, and a held tick asks the column
nothing. Each block keeps both:

  1  `commit_frame`: theirs' call with ours' borrow.
  2  `commit_frame_led`'s doc: ours' three lines, theirs' sentence on `held`,
     theirs' comment and lint allowance.
  3  `commit_counted_held`: theirs' `if !held`, ours' catalog clock inside it
     (`Shell::catalog_now` only reads the clock).
  4  `commit_tick`: theirs' comment and call with ours' borrow, then ours'
     card frame, passed the tick's `frame_held`. A held frame lands no band,
     so no preview is shown in a held tick (`preview_shown` is set by
     `Shell::band_landed`, called only from `landed_band`): the card's frame
     never runs in one, and if it did it would be held with its tick's.

    python3 -I -B eventloop.py <worktree-file>
"""
import hashlib
import sys

path = sys.argv[1]
B1 = """    commit_frame_led(
        shell,
        band,
        out,
        failures,
        now,
        &reconciled,
        &mut None,
        false,
    )"""
B2 = """/// left there by a tick that wrote no such update. It borrows the token its
/// caller took: [`commit_tick`]'s two frames, the preview's and its card's,
/// are written against one reconcile, with no wait between them. With
/// `held`, the frame waits for a folder listing's rows ([`commit_or_hold`])
/// and only what is not the frame is written.
// The frame's lead and whether the frame is held are the tick's to say, and
// the rest is every commit's: a struct for two flags would be one more thing
// for the other callers to build.
#[allow(clippy::too_many_arguments)]"""
B3 = """    // ([`Shell::settle_column`]); a tick that owes no frame asks nothing, and
    // neither does one whose frame is held. On the catalog's clock, which
    // counts sleep, not the frame's.
    if !held {
        let catalog_now = shell.catalog_now();
        shell.settle_column(catalog_now);
    }"""
B4 = """    // A held frame has no lead to ride: its title goes out on its own, ahead
    // of the report here as it would be ahead of it there.
    commit_frame_led(
        shell,
        band,
        out,
        failures,
        now,
        &reconciled,
        &mut lead,
        frame_held,
    )?;
    // The card a preview's frame just showed, in the frame after it and in
    // this tick, as fx builds its card's frame once its preview's frame has
    // committed (`Shell::take_preview_shown`). Nothing is read and no resize
    // is taken between the two: the loop reads and takes a resize only in
    // `collect_facts` and `read_burst`, ahead of this commit (`run`). The
    // facts this tick reconciled are the ones the card is written against.
    // A held frame lands no band, so it showed no preview.
    if shell.take_preview_shown() {
        commit_frame_led(
            shell,
            band,
            out,
            failures,
            now,
            &reconciled,
            &mut None,
            frame_held,
        )?;
    }"""
BLOCKS = [
    (("098df1424edc80ee", "6787459b6d5ff996", "e876e6b91d6eb1c9"), B1),
    (("000b3ef529883d1f", "86b431a9c94a9cd2", "7f77ba74f85d0a05"), B2),
    (("94902f2a5c6e9128", "4dc418b6e4d69cb5", "dac1fe42cec4b45b"), B3),
    (("798cff9dc43554b2", "292896cf8e7ca7a6", "23255964608601d2"), B4),
]


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]


lines = open(path, encoding="utf-8").read().split("\n")
out, i, n = [], 0, 0
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    bar = next(j for j in range(i, len(lines)) if lines[j].startswith("||||||| "))
    mid = next(j for j in range(bar, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    sides = (h(lines[i + 1:bar]), h(lines[bar + 1:mid]), h(lines[mid + 1:end]))
    if n >= len(BLOCKS) or sides != BLOCKS[n][0]:
        sys.exit(f"{path}:{i + 1}: block {n + 1} is not the one read by hand {sides} -- read by hand")
    at = len(out) + 1
    out.extend(BLOCKS[n][1].split("\n"))
    print(f"{path}:{at}: block {n + 1} resolved by hand (ours {bar - i - 1} lines, base {mid - bar - 1}, theirs {end - mid - 1} -> {len(BLOCKS[n][1].splitlines())})")
    n += 1
    i = end + 1
if n != len(BLOCKS):
    sys.exit(f"{path}: {n} blocks, not {len(BLOCKS)} -- read by hand")
open(path, "w", encoding="utf-8").write("\n".join(out))
print(f"# {path}: {n} blocks resolved")
