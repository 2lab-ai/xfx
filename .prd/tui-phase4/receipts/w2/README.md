# W2 receipt: /image (/img), /images, /paste and Ctrl-V

Nine scenarios, run at 40x120 on both binaries through the repo harness -- the
first four below, two in the follow-up section, two in round 3, one in round 4.
`image-paste`, `image-inline` and `image-composer` were retaken in round 4, once
the painter drew links: the files here are those, and round 4 says how they
compare. The tables in the earlier sections describe the first takes, when
every `.ansi` difference was the badge's missing link:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

with `TMPDIR=/private/tmp/claude-501`, for `image`, `image-unknown`,
`image-nonnative` and `image-paste` (`scripts/fxcap/scenarios/`). The workspace
is `scenarios/image-ws/`: fx's capture fixture `shot.png` (a 4x4 red PNG, 73
bytes, sha256 `3fd6e6be528c…`), `notes.txt` (text) and a directory `pics/`.

- `image.cap` (catalog `models-image.json`: `fake/model-a` tagged `tool-use`,
  `vision`, `file-input`; turns `image-turns.json`): every refusal, `/images`
  empty, one attach, `/images` typed after the badge, `/images clear`, an attach
  dropped by Ctrl-U, `/img` and `/image` typed after the first badge, the prompt
  that sends both, a second prompt, `/quit`, a relaunch that resumes the
  session (`fx: --resume-last`, `xfx: --resume last`), and a third prompt.
- `image-unknown.cap` (catalog `models-empty.json`, no `fake/model-a`): attach,
  then a prompt.
- `image-nonnative.cap` (the harness's default catalog, `tool-use` only): attach,
  then a prompt.
- `image-paste.cap`: `/paste` and Ctrl-V with the harness's do-nothing
  `osascript`, then the same with `clipimage image-ws/shot.png`, `/images` after
  the badges, and a prompt.

The xfx binary is `target/debug/xfx` built at `694eb48`: this branch with
`plan/tui-phase4` at `acdb2b5` merged (W1 `/usage`, W3 `/trace`, D4d
`/settings` among what came in); its banner says `v0.1.0-694eb48`. fx is the
c1db919 build (banner `v0.0.13`). Every capture here was taken on that tree
with that harness. Two earlier sets -- at `edafd5d` (plan `d76c10a`) and
before the replay-summary fix -- said the same thing row for row, apart from
the banner, wall-clock seconds, and the replayed summary's `↑11` the fix
changed to fx's `↑12`.

## What was added to capture.sh

- `sparsepng <name> <bytes>`: a file of `<bytes>` in the workspace that begins
  with the PNG signature and is a hole after it. `image.cap` makes `huge.png`
  at 22020096 bytes (21 MiB) this way, so the repository holds no 21 MiB file.
- `pnghdr <name> <width> <height>` (the follow-up): a PNG signature and an
  `IHDR` declaring `<width>` x `<height>`, 24 bytes -- an image over a pixel
  limit, as fx's own tests make one.
- `pastex <bytes>` (round 3): a bracketed paste of `<bytes>` `x`s through a
  file in the run directory -- a paste too large for a command line.
- `clipimage <file>|off`: the `osascript` shim now copies `<file>` to the path a
  clipboard-image script names (`-e` with `POSIX file "<path>"` and
  `class PNGf`, which is the script both fx and xfx send) and does nothing else
  for any other call; `off`, the default, is the old shim exactly (writes
  nothing, exits 0). This is the only way a success receipt for `/paste` can be
  taken without the real clipboard.
- The `osascript` shim is run once before the session, as `pbcopy` already was.
  fx runs `osascript` on its UI thread for `/paste`; without the warm-up its
  first `/paste` landed after the scenario's snap (the slash menu was still up
  1.5 s after Enter), because macOS assesses a new executable the first time it
  runs. W3 (`/trace`) made the same fix on plan; the merge keeps its
  `osascript warm-up` and one shim for both units: `clipfail on` makes it exit
  1, a clipboard-image script writes the `clipimage` file (and is never kept
  for W3's `clipfile`), and any other call is kept for `clipfile` as W3's
  was.

The other shims (`open`, `pbcopy`, `pbpaste`, `afplay`), `FX_NO_OPEN_BROWSER=1`
and the fx sound guard are unchanged; every fx run ended with all seven chime
names still directories (exit 0). `ls /private/tmp | grep fx-image-snapshots`
and `ls /private/tmp/claude-501 | grep xfx-clipboard` were 0 before and after
the capture runs: neither binary left its clipboard directory behind.

**Sanitized**: the run directory (`/private/tmp/claude-501/fxcap.XXXXXX`) is
written as `<scratch>`. It appears in fx's `.ansi` files (the badge's OSC 8
link, `file://<scratch>/work/shot.png`) and in both `.files` dumps. Every file
was then scanned for `fxcap.`, `/private/`, `/var/folders`, `claude-501`,
`/Users/` and the fake key: no match. The raw request bodies are **not**
committed -- their system messages carry the workspace path and their file
parts the base64. `<bin>-<scenario>-40x120-wire.json` is what they say about
this unit instead (`wire.py` in the session scratchpad): per request, every
user message's parts -- text as written; a file part as its `mediaType`, the
length of its base64 and the SHA-256 of the bytes it decodes to -- plus
`toolChoice` and the advertised tool names.

## Per snap

Row 1 is each binary's banner and differs in every snap; "identical" below
means rows 2-40.

### `image`

| snap | fx | xfx | matches |
|---|---|---|---|
| `bare` (`/image`) | `✗ usage: /image <path>` | same | Identical, `.txt` and `.ansi`: `\e[38;5;252m✗ \e[38;5;250musage: /image <path>\e[39m` -- the error's glyph and no topic (`image_commands.zig:24-27`). |
| `missing` (`/image nope.png`) | `✗ images: image file not found` | same | Identical, `.txt` and `.ansi`. |
| `unsupported` (`/image notes.txt`) | `✗ images: unsupported image type` | same | Identical, `.txt` and `.ansi`. |
| `toolarge` (`/image huge.png`, 21 MiB, PNG signature) | `✗ images: image exceeds the 20 MiB limit` | same | Identical, `.txt` and `.ansi`. Refused by its size, before its bytes are read. |
| `directory` (`/image pics`) | `✗ images: failed to attach image: NotRegularFile` | same | Identical, `.txt` and `.ansi`. |
| `images-empty` (`/images`) | `* images: no pending images` | same | Identical, `.txt` and `.ansi`. |
| `attached` (`/image shot.png`) | `* images: attached image: shot.png`, composer `┃ [Image 1]` | same | `.txt` identical. `.ansi`: the notice row is identical; the composer row differs only by fx's OSC 8 link around the badge (`\e]8;;file://<scratch>/work/shot.png\e\[Image 1]\e]8;;\e\`), which xfx does not draw -- see "Differences". |
| `images-listed` (` /images` after the badge) | `* images: 1 pending` / `   - shot.png (image/png)`, composer keeps `┃ [Image 1]` | same | `.txt` identical; `.ansi` identical but the badge's link. The command typed after the badge ran with the badge kept. |
| `images-cleared` (` /images clear`) | `* images: cleared pending images`, composer empty | same | Identical, `.txt` and `.ansi`. |
| `cu` (attach, then Ctrl-U) | composer empty | same | Identical, `.txt` and `.ansi`. |
| `images-after-cu` (`/images`) | `* images: no pending images` | same | Identical, `.txt` and `.ansi`: deleting the badge dropped the attachment. |
| `two-attached` (`/img shot.png`, then `/image shot.png` typed after its badge) | two `attached image: shot.png` notices, composer `┃ [Image 3][Image 4]` | same | `.txt` identical, numbers included -- #1 cleared and #2 dropped are never reused. `.ansi` identical but the link. |
| `sent` (` what is in this image?`) | card `┃ [Image 3][Image 4] what is in this image?`, the answer, `0s (↑12 ↓40)` | same | `.txt` identical (an earlier run on a loaded host said `1s` here: a summary's seconds are wall-clock). `↑12` is the estimate of `[Image #3][Image #4] what is in this image?`, the prompt the model was sent, as fx counts it. `.ansi`: the card is bold on both; only the link differs. |
| `again` (`describe it again`) | the second turn | the same rows, two blank rows more between `┃ describe it again` and `Still red.` | **Differs, and not in this unit.** The second turn runs on a full screen; the activity row grows the band and carries the document up (`Band::carry_document`), and the rows it gives back when the turn ends are blank -- the band geometry F1's receipt describes for a closed approval (`receipts/f1/README.md`, "Blank rows after a closed approval"). This turn carries no image. The two extra rows push one more row of the top into scrollback, which is why `end.full` has one more leftover row above the resume. |
| `resumed` (relaunch, resume) | `* session resumed: [Image #3][Image #4] what is in this image?`, then the two turns replayed, the first card `┃ [Image 3][Image 4] what is in this image?` with `0s (↑12 ↓40)` | same | `.txt` identical; `.ansi` but the replayed card's link (fx links a replayed badge too). The resume notice names the session by its first prompt as the log holds it (tokens), the replayed card shows badges, and the summary counts the prompt that was sent (fixed in `edafd5d`; it said `↑11` before, the count of the badges). |
| `resumed-sent` (`once more`) | the third turn | same | `.txt` identical; `.ansi` but the first card's link. |

Wire (`fx-image-40x120-wire.json`, `xfx-image-40x120-wire.json`): three
completion requests on each side, and their user messages are **the same**:
request 1 `[Image #3][Image #4] what is in this image?` then two `file` parts
`image/png`, 100 base64 bytes each, decoding to sha256 `3fd6e6be528c…` (the
file); request 2 the same first message -- both images sent again -- then
`describe it again`; request 3, after the resume, both images again, then the
two later prompts. The advertised tools differ (fx 16, xfx 9): each product's
own registry.

Durable (`fx-image-40x120-end.files`, `xfx-image-40x120-end.files`): fx keeps
`images/image-2-…`, `image-3-…`, `image-4-…` beside its session log and names
them in its `user` event (`"images":[{"id":3,…,"snapshot_path":"images/image-3-3fd6e6be528c182d.bin",…}]`).
xfx keeps `images/image-3-3fd6e6be528c182d.bin` and `image-4-…` (73 bytes,
owner-only) and records them in an `images_attached` event after the
`user_message` (`{"id":3,"path":"<scratch>/work/shot.png","media_type":"image/png","sha256":"3fd6…","bytes":73}`).
fx's `image-2` is the attachment Ctrl-U dropped, whose copy fx keeps; xfx
writes a copy only when a prompt sends the image, so a dropped one leaves none.

### `image-unknown` (the catalog does not list the model)

| snap | fx | xfx | matches |
|---|---|---|---|
| `attached` | `* images: attached image: shot.png`, `┃ [Image 1]` | same | `.txt` identical; `.ansi` but the link. |
| `sent` | card, then `✗ system: Unable to verify image support for this model, so the image was not sent. Try again later, choose another` / `  model, or remove the image.` | same | `.txt` identical. `.ansi`: the card's link, and xfx's first notice row ends `another \e[39m` where fx's ends `another\e[39m` -- xfx's notice wrap keeps the space it broke at as the row's last cell. That is the notice writer's, not this unit's; it is invisible and reported separately. |

Wire: **no completion request on either side.** fx sends only its title
request (`001-title.json`, text only); xfx has no title request, so its
gateway log has the catalog read and nothing else. The catalog was read once
on both (`GET /coding-agent/v1/models bearer=yes`).

### `image-nonnative` (the catalog lists the model with `tool-use` only)

| snap | fx | xfx | matches |
|---|---|---|---|
| `attached` | as above | same | `.txt` identical; `.ansi` but the link. |
| `sent` | card, the model's text `It is a small red square.`, then `✗ system: request failed: RequiredVisionToolCallMissing` | card, then `✗ system: request failed: SubscriptionNativeImageUnavailable` | **Differs by design.** fx routes a listed model without `vision`+`file-input` through a forced `vision` tool call (`toolChoice: required`, the user text with `<available_images>` appended and no file part -- `fx-image-nonnative-40x120-wire.json` request 2); the fixture answers with text, so fx fails the turn after it. xfx does not port that route and refuses the model before any request, in fx's own words for that route when it is unavailable (`orchestrator.zig:7152-7155`). Recorded in `ssot.md` §Tensions. |

Wire: fx a title request and one completion request (`toolChoice`
`required`, one tool, no file part); xfx none.

### `image-paste`

| snap | fx | xfx | matches |
|---|---|---|---|
| `paste-none` (`/paste`, the do-nothing shim) | `✗ images: failed to paste clipboard image: FileNotFound` | same | Identical, `.txt` and `.ansi`. The shim says it wrote the file and did not; both then fail to read it, by fx's error name. |
| `ctrl-v-none` (Ctrl-V, the same) | the same row again | same | Identical, `.txt` and `.ansi`. |
| `paste` (`/paste`, `clipimage`) | `┃ [Image 1]`, **no notice** | same | `.txt` identical. `.ansi`: fx links the badge to the session copy (`file://<scratch>/home/.fx/sessions/<id>/images/image-1-….bin`); xfx draws no link. |
| `ctrl-v` (Ctrl-V, `clipimage`) | `┃ [Image 1][Image 2]`, no notice | same | `.txt` identical; `.ansi` but the links. |
| `images` (` /images`) | `* images: 2 pending` / `   - image-1-3fd6e6be528c182d.bin (image/png)` / `   - image-2-3fd6e6be528c182d.bin (image/png)` | same | `.txt` identical: a clipboard image is named by its session copy on both. `.ansi` but the links. |
| `sent` (`what is this?`) | card `┃ [Image 1][Image 2] what is this?`, answer, `0s (↑10 ↓40)` | same, `1s (↑10 ↓40)` | Identical but the summary's wall-clock seconds and the links. |

Wire: one completion request on each side with the same user message --
`[Image #1][Image #2] what is this?` and two `image/png` file parts decoding to
`3fd6e6be528c…`; fx's title request besides.

## Follow-up: paths in a prompt, and withholding at send

Two more scenarios, the same harness and binaries' kinds: xfx is
`target/debug/xfx` at `4774a9e` (banner `v0.1.0-4774a9e`), fx the c1db919
build. The earlier snaps above were not retaken.

- `image-inline.cap`: a bracketed paste of `shot.png describe this too`,
  Enter, then `inspect shot.png,` typed and sent, then a paste of
  `see nope.png and notes.txt`. It waits five seconds before the first paste:
  a run that pasted at three seconds on a loaded host lost the paste on xfx,
  which had not yet asked for bracketed paste.
- `image-withheld.cap`: `wide.png` made at launch by the harness's new
  `pnghdr wide.png 9000 10` -- a PNG signature and an `IHDR` declaring 9000x10,
  fx's own `testPngHeader` (`image_data.zig:370-379`) -- attached with
  `/image`, then ` describe` sent.

### `image-inline`

| snap | fx | xfx | matches |
|---|---|---|---|
| `pasted` | composer `┃ [Image 1] describe this too`, no notice | same | `.txt` identical; `.ansi` but the badge's link. |
| `pasted-sent` (Enter) | card `┃ [Image 1] describe this too`, the answer, `0s (↑7 ↓40)` | same | `.txt` identical; `.ansi` but the link. |
| `typed` (`inspect shot.png,` typed, not sent) | composer `┃ inspect shot.png,` | same | Identical but the first card's link: a typed path stays words until the prompt is sent. |
| `typed-sent` (Enter) | card `┃ inspect [Image 2],`, `Still red.`, `0s (↑5 ↓40)` | same | `.txt` identical; `.ansi` but the links. The comma is kept after the badge. |
| `pasted-missing` (`see nope.png and notes.txt` pasted) | composer `┃ see nope.png and notes.txt`, no notice | same | `.txt` identical; `.ansi` but the earlier cards' links. A path that names no file is the words it is. |

Wire (`*-image-inline-40x120-wire.json`): request 1 `[Image #1] describe this
too` and one `image/png` file part (100 base64 bytes, sha256 `3fd6e6be528c…`);
request 2 that message again, then `inspect [Image #2],` and its file part.
Identical on both; fx sends a title request besides.

### `image-withheld`

| snap | fx | xfx | matches |
|---|---|---|---|
| `attached` | `* images: attached image: wide.png`, `┃ [Image 1]` | same | `.txt` identical; `.ansi` but the link. |
| `sent` | card `┃ [Image 1] describe`, `It is a small red square.`, `0s (↑5 ↓40)` | same | `.txt` and `.ansi` identical but the link. The note is the model's: the screen shows the turn as typed. |

Wire (`*-image-withheld-40x120-wire.json`): one completion request on each
side, **no file part**, and one text part:

```
[Image #1 not sent: image/png is 9000x10 pixels. This request permits at most 8000 per side and 5 MiB encoded per image. The original is saved at <scratch>/home/.xfx/sessions/<id>/images/image-1-c27bf7313d5f0cbe.bin. Use an available image tool to save a smaller copy to a new file ending in .png, then read_file the copy. If no image tool is available, ask the user before installing one.]
[Image #1] describe
```

The same bytes on both but the saved copy's directory (`.fx` against `.xfx`,
and each product's session id). Durable (`*-image-withheld-40x120-end.files`):
both keep the 24-byte copy as `images/image-1-c27bf7313d5f0cbe.bin`, and xfx's
log holds the turn as typed -- `user_message` `[Image #1] describe` and its
`images_attached` -- with no note in it. xfx's gateway log ends with the fake
gateway's `stream closed by client` line, as several earlier xfx receipts' do
(`e1`, `f1`, `f3`): the harness's, after the reply was read.

**What the note asks for that xfx cannot do.** fx's note tells the model to
`read_file the copy`, and fx's `read_file` returns an image
(`read_file.zig:799`); xfx's names a binary file without reading it. The
sentence is fx's, word for word; `ssot.md` §Tensions records the gap.

## Round 3: the badge in the composer's edits, and `read_file` of an image

Two more scenarios; xfx is `target/debug/xfx` at `d84afb4` (banner
`v0.1.0-d84afb4`), fx the c1db919 build.

- `image-composer.cap` (turns `image-composer-turns.json`): a pasted
  `shot.png` and ` duplicate [Image #1]` typed after it, sent; a pasted
  `shot.png` killed with Ctrl-U and yanked twice, sent; Up and Enter; a
  bracketed paste of 8388609 `x`s (one byte past the 8 MiB budget) through
  the harness's new `pastex`; then `RECOVERY` sent.
- `image-readfile.cap` (turns `image-readfile-turns.json`): the model calls
  `read_file` on `shot.png`, then on `wide.png` (made by `pnghdr`, 9000x10).

### `image-composer`

Every `.txt` is identical after the banner, and every `.ansi` is identical
once fx's OSC 8 links around the badges are taken out (T5's unit).

| snap | both | upstream |
|---|---|---|
| `lookalike` | `┃ [Image 1] duplicate [Image #1]` | |
| `lookalike-sent` | card `┃ [Image 2] duplicate [Image #1]`: the badge renumbered, the words kept | `mapSubmittedImages`, `input_submit_runtime.zig:1615-1692` |
| `yanked` | `┃ [Image 4][Image 5]`: #3 killed, yanked back twice as two images | `kill_ring.zig:120-190` |
| `yanked-sent` | card `┃ [Image 4][Image 5]`, `Two red squares.` | |
| `recalled` | `┃ [Image 6][Image 7]`: the recalled line's images under fresh numbers | `tui-composer-edit-contracts.test.ts:852-878` |
| `recalled-sent` | card `┃ [Image 6][Image 7]`, `Red, again.` | |
| `oversized` | `✗ input: That edit exceeds the local prompt safety limit and was not applied.` (`\e[38;5;252m✗ input:\e[38;5;250m That edit … applied.\e[39m`), the composer empty | `input_limit_feedback.zig:5-42` |
| `recovered` | card `┃ RECOVERY`, `Recovered.` | |

Wire (`*-image-composer-40x120-wire.json`): the same four requests on both
(fx numbers them from 002 behind its title request): `[Image #2] duplicate
[Image #1]` with one file; then `[Image #4][Image #5]` with two; then
`[Image #6][Image #7]` with two; then `RECOVERY`, each request carrying the
earlier turns' images again. **There is no flash**: fx answers the oversized
paste with that one notice row, which is what xfx now writes.

### `image-readfile`

| snap | fx | xfx | matches |
|---|---|---|---|
| `read` | `● 1 tool call · 1 read` / `└ Read shot.png`, `A small red square.`, `0s (↑6 ↓80)` | same | `.txt` and `.ansi` identical (retaken in round 5; at `d84afb4` xfx said `↓40`, the last request's count, where fx adds both requests' 40). |
| `wide` | the second turn, `Read wide.png`, `Too wide.`, `0s (↑4 ↓80)` | same | As above. |

Wire (`*-image-readfile-40x120-wire.json`), the requests after each read:

- `shot.png`: the result is a `content` output whose text is `<path>shot.png</path>\n<content>image attached (image/png, 73 bytes)</content>`, and a user message follows the results with `Attached image(s) from the tool result.` and one `image/png` file part (sha256 `3fd6e6be528c…`). The same on both, and the next turn's requests carry it again.
- `wide.png`: a `text` output, `image not attached: image/png (24 bytes); the image exceeds 8000 pixels per side. Use an available image tool to save a smaller copy to a new file, then read_file the copy. If no image tool is available, ask the user before installing one.`, and no image. The same on both.
- Two differences, neither the image: fx's attached result begins `Saved images: image-result-read_file-….txt. Use read_tool_result with this handle to load them again.` -- fx stores a result's images under a handle its `read_tool_result` tool reads back (`retainToolImages`, `execution_memory.zig:672-688`); xfx has no `read_tool_result`, keeps the copy beside its log (`tool_images_attached`) and sends it again itself, so the sentence would name a tool the model does not have. And fx sends a `Session file evidence from previous tool execution…` user message before the next prompt, which xfx has never sent.

## Round 4: links, and the amendment's refusal

xfx `target/debug/xfx` at `eb375fa` (plan `13dac71` merged: T5's OSC 8
links), fx the c1db919 build. Retaken: `image-paste`, `image-inline`,
`image-composer`. New: `approvals-amend-limit.cap` -- the edit's question, Tab
to amend, `keep` typed, a paste of 4097 `x`s, a second one, ` this` typed, a
third one, then Escape twice.

Compared row by row with the run directory written `<scratch>` on both and fx's
`.fx` and xfx's `.xfx` taken as one name:

| scenario | `.txt` | `.ansi` |
|---|---|---|
| `image-inline` (5 snaps) | identical | **identical, links included**: `\e]8;;file://<scratch>/work/shot.png\e\[Image 1]\e]8;;\e\` on the composer and on each card |
| `image-composer` (8 snaps) | identical | identical in 6. In `yanked` and `recalled` the links go to the same file on every cell but one: fx leaves the space inside the first yanked or recalled badge (`[Image 4]`, `[Image 6]`) unlinked, and the runs are split differently. Each binary's frame diff rewrote different cells over the composer's previous contents, and tmux keeps a link per OSC 8 written; fx's unlinked space is a cell its writer did not rewrite. Cards and `*-sent` rows are identical. |
| `image-paste` (6 snaps) | identical | identical but **the target** of the clipboard badges' links: fx links `<scratch>/home/.fx/sessions/<id>/images/image-N-3fd6e6be528c182d.bin`, xfx `<scratch>/xfxtmp/xfx-image-snapshots-<random>/image-N-3fd6e6be528c182d.bin`. Both write the copy at the paste and link it (`image_commands.zig:138-147`); fx's session exists from launch, so its copy goes into the session, while xfx's session opens with the first prompt, so its paste copy goes where fx puts one with no session -- a private temporary directory, removed when the process ends (`session_store.zig:102-118`, `image_attachments.zig:240-262`). The session keeps its own copy once the prompt is recorded, and a resumed card links that. `ls /private/tmp/claude-501 \| grep xfx-image-snapshots` was 0 after the runs. |
| `approvals-amend-limit` (5 snaps) | the `✗ input:` rows identical: one after the first paste, two after the second, three after the third | the `✗ input:` rows identical: `\e[38;5;252m✗ input:\e[38;5;250m That edit exceeds the input limit and was not applied.\e[39m`. Every other difference is not this unit's: the second choice's wording and its explanation row (xfx's permission panel), `What can xfx do differently?` (the product's name). |

Wire: `image-inline` and `image-paste` send the same user messages and file
parts as before on both (`*-wire.json`), and `image-composer` the same four
requests as round 3.

## Round 5: F4 merged, `image-readfile` retaken

xfx `target/debug/xfx` at `ee00a1e` (plan's `e17dd02` merged: F4's steering
and T6's output count), fx the c1db919 build. `image-readfile.cap` now waits
ten seconds after each Enter, not five: with F4 a prompt typed while a turn
still runs steers that turn, and on a loaded host the second prompt was typed
before the first turn ended.

| snap | `.txt` | `.ansi` |
|---|---|---|
| `read` | identical: `0s (↑6 ↓80)` on both | identical |
| `wide` | identical: `0s (↑6 ↓80)`, then `0s (↑4 ↓80)` | identical |

The output count matches now: a turn's summary adds every request's output
tokens, as fx does. Wire: the same requests as round 3 (the image after
`shot.png`'s result, none after `wide.png`'s), with round 3's two differences,
`Saved images: …` and `Session file evidence…`.

## Round 6: plan `20d1482` merged, an undo over a badge

xfx `target/debug/xfx` at `5de23de` (banner `v0.1.0-5de23de`; plan
`20d1482` merged as `c0d89f2`), fx the c1db919 build. New:
`image-undo.cap` -- `look` and a space typed, `shot.png` pasted, Backspace,
Ctrl-_ (undo) twice; then `shot.png` pasted again, Ctrl-U, Ctrl-Y, Ctrl-_.

| snap | both | upstream |
|---|---|---|
| `pasted` | `┃ look [Image 1]` | |
| `deleted` | `┃ look `: Backspace at the badge takes it whole | `composer_deletion.zig:300-329` |
| `undone` | `┃ look `, nothing said: the deletion was a boundary of the edit history, as the paste was, so the two undos find nothing | `composer_replacement.zig:223-232`, `edit_history.zig:99-101`, `app_input_runtime.zig:397-402` |
| `yanked` | `┃ look [Image 3]`: the line killed with its badge, yanked back under a fresh number | `kill_ring.zig:120-190` |
| `yank-undone` | `┃ look [Image 3]`: the kill and the yank were boundaries too | `composer_kill_ring.zig:44-118` |

Every `.txt` and `.ansi` is identical after the banner, the badges' links
included. Before `5de23de` the undo in `undone` brought back `[Image 1]` as
words with no image behind them (`deleting_the_badge_drops_the_image_and_an_undo_after_it_changes_nothing`
failed on the old rule: `left: "look [Image 2]"`, `right: "look "`).

## Round 7: plan `23907ead` merged, the boundary rule for pasted-text blocks

xfx `target/debug/xfx` at `eab9e5b` (banner `v0.1.0-eab9e5b`; plan
`23907ead` merged as `3373442`), fx the c1db919 build. New:
`paste-block-undo.cap` -- `look` and a space typed, 1001 `x`s pasted (a
block), Backspace, Ctrl-_ twice; 1001 `x`s pasted again, Ctrl-U, Ctrl-Y,
Ctrl-_, Ctrl-_.

| snap | fx | xfx | upstream |
|---|---|---|---|
| `pasted` | `┃ look [Pasted text #1, 1 line]` | `… 1 lines]` | the paste is a boundary, `input_paste_runtime.zig:436-438` |
| `deleted` | `┃ look ` | same | Backspace takes the block whole |
| `undone` | `┃ look `: the two undos find nothing | same | `composer_replacement.zig:223-232`, `composer_undo.zig:252-268` |
| `pasted-again` | `┃ look [Pasted text #2, 1 line]` | `… 1 lines]` | |
| `killed` | `┃` | same | the kill is a boundary, `composer_kill_ring.zig:90-118` |
| `yanked` | `look ` and the 1001 `x`s as text, no block | same text; the rows wrap differently | the ring holds the block's text, `kill_ring.zig:429-447` |
| `yank-undone` | `┃`: the yank was an entry | same | |
| `undone-again` | `┃`: the kill was not | same | |

Five of eight snaps are identical in `.txt` and `.ansi`. The other three
differ in two things that are not the undo:

- **The summary's noun.** fx writes `1 line` for one line
  (`formatPlaceholder`, `pasted_blocks.zig:59-62`); xfx's `paste::summary`
  always writes `lines` (`src/tui/paste.rs:326-328`).
- **Where a long word breaks in the composer.** fx keeps `look` and starts
  the run of `x`s on the same row, breaking it at the edge; xfx moves the run
  to the next row. The yanked text is the same 1006 characters on both.

Before `eab9e5b` xfx's `yanked` was `┃ look [Pasted text #3, 1 lines]` (a new
block), and the second undo in `undone-again` brought back
`┃ look [Pasted text #2, 1 lines]`, the killed block (a capture at `3373442`,
not kept).

## Differences, and why

- **Where a clipboard badge's link goes** (round 4): fx's copy is in the
  session it opened at launch, xfx's in a temporary directory until the prompt
  is recorded. Rounds 1-3's other badge-row `.ansi` differences were the link
  itself, which xfx did not draw before round 4.
- **The vision-tool route** (`image-nonnative`), above and in `ssot.md`.
- **No title request**: xfx never had one; not this unit's.
- **Wall-clock seconds** in turn summaries: timing.
- **`again`'s two blank rows**: band geometry after an activity row on a full
  screen, as F1's receipt records; the turn carries no image.
- **The trailing space in a wrapped notice row** (`image-unknown` `sent`
  `.ansi`): the notice writer's.
- **The paste wait** (`image-inline`): five seconds, not three, before the
  first paste, so it lands after both programs have asked for bracketed
  paste.
