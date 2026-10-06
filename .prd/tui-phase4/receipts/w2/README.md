# W2 receipt: /image (/img), /images, /paste and Ctrl-V

Four scenarios, run at 40x120 on both binaries through the repo harness:

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

## Differences, and why

- **The badge is not an OSC 8 hyperlink.** fx wraps `[Image N]` in a link to
  its file (`writeImageBadge`, `image_attachments.zig:1336-1340`). xfx's painter
  draws no hyperlink of any kind yet -- the open F5 links row in `loop.md`, a
  check-vocabulary decision -- so the badge is its text. Every `.ansi`
  difference in the badge rows is this and nothing else.
- **The vision-tool route** (`image-nonnative`), above and in `ssot.md`.
- **No title request**: xfx never had one; not this unit's.
- **Wall-clock seconds** in turn summaries: timing.
- **`again`'s two blank rows**: band geometry after an activity row on a full
  screen, as F1's receipt records; the turn carries no image.
- **The trailing space in a wrapped notice row** (`image-unknown` `sent`
  `.ansi`): the notice writer's.
