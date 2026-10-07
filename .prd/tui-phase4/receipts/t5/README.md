# T5 receipt: OSC 8 hyperlinks, as fx draws them

Two scenarios, run at 40x120 on both binaries through the repo harness:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<xfx b12eb45 debug>       scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

with `TMPDIR=/private/tmp/claude-501`, for `links` and `links-image`
(`scripts/fxcap/scenarios/`). The xfx binary is `target/debug/xfx` built at
`b12eb45` -- this branch with `plan/tui-phase4` at `1fc2ea4` merged; its
banner says `v0.1.0-b12eb45 [debug]`. fx is the c1db919 build (banner
`v0.0.13`). Both runs used the harness as it is on this branch, with its
shims, `FX_NO_OPEN_BROWSER=1` and its fx sound guard (every fx run exited 0
with the chime names still directories).

- `links.cap` (turns `links-turns.json`): one answer carrying an inline link,
  a bare URL, an angle autolink and an email autolink, a URL in a code span,
  three links whose targets xfx does not link (`README.md`,
  `javascript:alert(1)`, `ftp://example.com/f`), a link whose text wraps past
  the row's edge, and a link in a table cell; then the full transcript
  (Ctrl-O, closed with Ctrl-O), a second prompt whose answer's link takes the
  next id, `/quit`, and a relaunch that resumes the session
  (`fx: --resume-last`, `xfx: --resume last`).
- `links-image.cap` (catalog `models-image.json`, turns `image-turns.json`,
  workspace `image-ws/`): `/image shot.png`, a prompt that sends it, `/quit`,
  and a relaunch that resumes it.

**Sanitized**: the run directory (`/private/tmp/claude-501/fxcap.XXXXXX`) is
written as `<scratch>`; it is in both binaries' badge links
(`file://<scratch>/work/shot.png`). Every file was then scanned for `fxcap.`,
`/private/`, `/var/folders`, `claude-501`, `/Users/`, `AI_GATEWAY` and the
fake key: no match. No request body is committed (neither scenario asks for
one).

**How to read the `.ansi` files.** They are `tmux capture-pane -e`: tmux's
own spelling of the cells it holds, not either binary's bytes. tmux keeps a
hyperlink per cell, with the `id=` a link was opened with, and writes it back
as `ESC ] 8 ; id=<id> ; <uri> ESC \` ... `ESC ] 8 ; ; ESC \` around each run.
So an identical `.ansi` row means the same text, the same attributes and the
same link -- target and id -- in every cell.

**Link ids.** fx's are `fx-N` from one counter for the whole process
(`var link_id_counter: u32 = 1`, `src/core/agent/assistant_presentation.zig:13`),
and xfx's presenter numbers them the same way, so in these captures -- one
fresh process per launch -- the ids are the same on both sides, `fx-1` to
`fx-11`, and after the relaunch `fx-1` again on both. Within one process the
ids only agree while both have rendered the same answers: xfx's full
transcript shows the presenter's own output, ids included, as fx's does.

## What xfx writes for a link

The bytes the document append writes for the first link of the answer
(`frame::place`: `CUP`, the row as `frame::row_text` admits it, then `EL`) are
the presenter's, unchanged:

```
\e]8;id=fx-1;https://example.com/docs\e\\\e[38;5;75m\e[4mthe docs\e[24m\e[39m\e]8;;\e\\
```

A band row (the composer's badge) goes through the grid diff, which opens a
link after the attributes and closes it before the next row's `CUP`:

```
\e]8;;file://<scratch>/work/shot.png\e\\[Image 1]\e]8;;\e\\
```

## Per snap

Row 1 is each binary's banner in every snap but `resumed`, where both screens
were reset and row 1 is the resume notice; "identical" below means every row
after the banner (every row, for `resumed`).

### `links`

| snap | fx | xfx | matches |
|---|---|---|---|
| `answered` | the answer, the dim summary `0s (↑5 ↓40)` | same | `.txt` identical but row 26, the summary's wall-clock seconds (`1s` on xfx this run). `.ansi` identical but row 26 and row 13, `Also readme, script and ftp.`: fx links all three (`id=fx-6;README.md`, `id=fx-7;javascript:alert(1)`, `id=fx-8;ftp://example.com/f`), xfx draws the same text in the same link colour and underline with no link -- see "Not matched". Every other link is the same cell for cell: `id=fx-1` `https://example.com/docs`, `fx-2` the bare `https://example.org/path`, `fx-3` `mailto:someone@example.com`, `fx-4` `https://example.net/a`, `fx-5` the code span's `https://example.com/code` (grey, not underlined, as fx's), `fx-9` the wrapped link on **both** of its rows -- each row closes it (`past\e[0m\e]8;;\e\`) and the next opens it again -- and `fx-10` inside the table cell. |
| `full` (Ctrl-O) | the full transcript | the full transcript | Every link-bearing row is identical, ids included (the presenter's own output, as fx's screen shows): the six rows with links on both, and row `Also …` linked on fx only, as above. The screen as a whole differs in rows that are not this unit's -- the session entry's lines (fx `skills: loading` / `mcp: none`, xfx `tools: 9 advertised (…)` over two rows), fx's second `Response` heading above `Done.`, the timestamps and the summary's seconds -- so the rows from the prompt down to `Done.` are one row lower on xfx, and level again after it. Those are F5's (`receipts/f5/`), not this unit's. |
| `second` (Ctrl-O again, then `and one more`) | the second turn, `Again: second.` on row 30 | same | `.txt` identical but row 26, the first summary's seconds. `.ansi` the same two rows, 13 and 26. The second answer's link is `id=fx-11;https://example.com/second` on both, identical: the counter went on from the first answer. |
| `second.full` (the whole tmux history) | | | Nothing had scrolled off yet, so it is `second` again: rows 13 and 26 only. |
| `resumed` (relaunch, resume) | `* session resumed: show me the links`, both turns replayed | same | `.txt` identical, every row. `.ansi` identical but row 13 (the three unlinked targets). The replay numbers its links from `fx-1` again on both -- a new process -- so the second answer's is `fx-11` again. |
| `resumed.full` | | | `.txt` identical; `.ansi` identical but row 19, the same `Also …` row. |

### `links-image`

| snap | fx | xfx | matches |
|---|---|---|---|
| `attached` (`/image shot.png`) | `* images: attached image: shot.png`, composer `┃ [Image 1]` | same | Identical, `.txt` and `.ansi`: the composer's badge is `\e]8;;file://<scratch>/work/shot.png\e\[Image 1]\e]8;;\e\` on both (`writeImageBadgeClipped`, `image_attachments.zig:1342-1354`). W2's receipt had this row differ by the link. |
| `sent` (` what is in this image?`) | card `┃ [Image 1] what is in this image?` in bold, the answer | same | Identical, `.txt` and `.ansi`: the card's badge is linked and the text after it is not (`expandPlaceholdersWithLinkedBadges`, `user_message_card.zig:222-230`). |
| `resumed` (relaunch, resume) | the replayed card | same | Identical, `.txt` and `.ansi`: the replayed card links its badge to the file the session log names. |

## Not matched

- **Targets outside four schemes are text** (`.ansi` row 13 of `answered`,
  `second`, `resumed`). fx links whatever target an inline link names: its
  only rule is the length and the control bytes (`isValidLinkUrl`,
  `src/core/agent/presentation/inline_render.zig:917-921`), so `README.md`
  (a relative reference), `javascript:alert(1)` and `ftp://…` are links
  there. xfx admits `http`, `https`, `file` and `mailto` only
  (`src/tui/hyperlink.rs`), because a terminal hands a clicked URI to
  whatever the system opens that scheme with, and the target is a model's
  choice. The text keeps fx's link colour and underline either way, and the
  presenter still spends the id (`fx-6`..`fx-8`), so every later id agrees.
- **A clipboard image's badge is text** (not captured here: `/paste` under the
  harness writes a file the fake clipboard names). fx links the copy it writes
  into the session at the paste (`fx-image-paste-40x120-paste.ansi` in W2's
  receipt: `file://<scratch>/home/.fx/sessions/<id>/images/image-1-….bin`);
  xfx writes its copy only when the turn is recorded, so at the paste there is
  no file to link, and the badge's path is the copy's name alone, which a
  `file://` link cannot carry.
