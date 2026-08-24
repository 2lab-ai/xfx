# fx input/composer/footer subsystem — research note for the xfx port

Source: local clone `scratchpad/fx-src` @ HEAD ef1d0d0 (vercel-labs/fx, Zig).
All paths below are relative to the repo root. Line numbers verified against this checkout.
Marked `[추정]` where I did not read the exact call site.

Architecture in one sentence: **UI owns terminal mechanics (escape decoding, visual layout, footer painting), Core owns semantics (editor state, entities, history, slash routing, approval/question state); the boundary is a typed event union** (`src/core/input/input_action.zig:128-132` `TerminalInputEvent = paste_byte | raw | action`).

---

## 1. Key decoding — bytes → semantic actions

### Terminal modes enabled
`src/ui/terminal/terminal.zig:4`:
```zig
pub const interactive_mode_enable_sequence = "\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l";
```
= xterm modifyOtherKeys level 2, **kitty keyboard protocol flag 1 (disambiguate escape codes)**, **bracketed paste (2004)**, autowrap off. tmux gets a variant *without* the kitty push (`terminal.zig:5`). Mouse tracking: the alternate-screen leave sequence disables `?1000`/`?1006` (`terminal.zig:13`), i.e. SGR mouse is enabled for the alternate/full-transcript screen [추정: enable is in the alternate-screen enter path; the parser handles SGR + legacy X10 regardless].

### The escape parser (`src/ui/input/escape_parser.zig`, 761 lines)
Hand-rolled byte-at-a-time state machine. State = one `stage: u8` (meta-prefix bit `0x80`, `escape_parser.zig:25,175-185`) + two `u16` params + a small `MouseInput` struct (`:7-17`). Stages:
- 1 = bare ESC; 2 = CSI intro; 3 = CSI first number; 4 = SS3 (`ESC O`); 5 = second param; 6 = third param (modifyOtherKeys `ESC[27;mod;key~`, `:641-657`); 7-9 = SGR mouse button/col/row; 10-12 = legacy X10 payload; 13/14 = mouse-discard; 15 = bounded unknown-CSI discard (max 32 bytes, `:37,234-278`); 16 = kitty event-type stage (`:35,658-675`).

Coverage, with evidence:
- **Arrows** plain CSI/SS3 `A-D` → `cursor_up/down/left/right` (`:456-464`, `:535-553`); `H/F` home/end; `CSI Z` (Shift-Tab) → `toggle_permission_mode` (`:467`, and with modifiers `:620-623`).
- **Modified arrows** `CSI 1;mod X` → `modifiedArrowAction` (`:52-88`): super = draft start/end; alt/meta = paragraph up/down, word left/right; ctrl = word/visual moves; **shift bit sets `extend_selection`** on the emitted `composer_shortcut` move (`composerMove`, `:45-50`).
- **Tilde keys** `1,3,4,5,6,7,8~` → home/delete/end/pgup/pgdn (`:526-533`); modified `3~` variants map to delete-word/line (`:589-594`).
- **Bracketed paste** `ESC[200~`/`201~` → `.paste_start`/`.paste_end`, only when exactly 3 digits (`:520-524`).
- **Kitty CSI-u** `kittyUnicodeKeyAction` (`:98-159`): keycode 27 = escape; kitty functional up/down keys 57352/57353 (`:38-39,102-111`); Shift/Alt+Enter (keycode 13 + mods) → `insert_newline` (`:112-114`); super+a/c/x/z(+shift) → select_all/copy/cut/undo/redo (`:116-127`); ctrl+letter remapped back to control bytes (`:148-152`); alt+b/f word moves; caps/num-lock bits stripped (`:100`). Event-type qualified reports (`ESC[27;1:1u`, Ghostty) accept only press/repeat — release must not close a panel (`:658-675`).
- **SS3 legacy** ESC-prefixed meta keys: `ESC b/f` word moves, `ESC d` delete-word-right, `ESC BS/DEL` delete-word-left, `ESC CR` newline (`:407-447`).
- **Mouse**: SGR `ESC[<btn;col;row M/m` — wheel if bit 64 (`sgrMouseAction` `:340-368`), else press/drag/release `MousePointer` with shift/alt/ctrl from bits 4/8/16; legacy X10 `ESC[M` 3-byte payload, **wheel only** (`:729-753`). Bounded: SGR max 18 bytes (`:36`), discard machinery for torn reports (`:280-332`).

### Decoder driver (`src/ui/input/terminal_action_decoder.zig`)
`Decoder.feed(byte, ctx)` (`:29-137`): paste-active bypasses decoding entirely (`:38-40` → `paste_byte` event); ESC begins capture and **snapshots `cancel_pending`** so Esc-as-cancel keeps its meaning even when resolved later (`:199-206`); bare-ESC followed by a control byte emits `.escape` **plus `replay_byte_after_routing`** so both events route in order (`:105-114`, `input_action.zig:143-160` — "one terminal byte produces at most one event" invariant with `setEvent` assert). `flush(now, timeout)` resolves a lone ESC to `.escape` after the quiet timeout; mouse payload stages get an extended 250ms timeout (`:7,148-151`). Unknown CSI resolves to `.ignore`, never a phantom Escape (test `:342-360`).

### Semantic action model (`src/core/input/input_action.zig`)
- `Action` union (`:72-103`): cursor/word/home/end/page moves, mouse_wheel/pointer, delete family, `toggle_full_transcript`, `toggle_permission_mode`, `open_all_sessions`, `insert_newline`, `paste_start/end`, `composer_shortcut: ShortcutAction`, `remapped_byte: u8`, `escape`, `ignore`.
- `ShortcutAction` (`:50-69`): move(MoveIntent), select_all, copy/cut selection, undo/redo, history prev/next, delete variants, yank, redraw, insert_newline. `MoveKind` (`:27-42`) includes `visual_up/down` (soft-wrap-aware) vs `paragraph_up/down` vs `draft_start/end`.
- Emacs control-byte table `src/ui/input/shortcuts.zig:11-30`: C-a/C-e/C-b/C-f = line/char moves, C-p/C-n = history, C-d, C-k, C-u, C-w (delete whitespace-word), C-y yank, C-_ undo, C-l redraw, DEL/BS backspace, C-j newline. Ctrl-O (byte 15) → `toggle_full_transcript` even outside escape context (`escape_parser.zig:161-166`).
- A raw byte event carries its composer fallback along (`terminal_action_decoder.zig:220-225`), so Core can route the byte to a higher-priority surface (approval/question/subagent) first and only then fall back to the composer shortcut (`input_action.zig:105-113`).

### Parallel surface interpretation (`src/ui/input/runtime.zig`)
`Runtime.decodeTerminalByte` (`:871-881`) wraps each ingress with **three additional typed interpretations** so Core can pick by focus owner:
- Approvals: `approvalActionFromByte` (`:65-77`) — Ctrl-C=deny, Enter=submit, Tab, BS, `1-3` = number, printable → `insert_ascii` (amendment draft); shortcut→`DraftAction` mapping (`:79-101`).
- Questions: `questionActionFromByte` (`:139-158`) — `1-9` ordinal select (or insert when freeform focused), Tab=next entry, Enter=submit, Ctrl-C=cancel; shift+visual-up/down = move choice (`:182-201`).
- Subagent panel: full editor-action remap (`:265-353`).
Gestures: Ctrl-C-twice exit window 3000ms, double-Esc clear window 500ms (`src/core/input/gesture_state.zig:3-4`), transitions are pure functions (`:52-118`). Core-side entry points: `src/core/app/app_input_runtime.zig` `handleTerminalByteWithLimits` (`:675`), `flushPendingEscape` (`:646`), paste routing (`:707,724`).

---

## 2. Editor — data model and operations

### Data model (`src/core/input/editor_state.zig`)
One flat UTF-8 `ArrayList(u8)` + byte `cursor` + optional `selection_anchor` (`:30-33`). No line array, no rope, no per-grapheme storage. Selection ops (`:61-146`): begin/extend/finish/clear, `collapseSelection(edge)`, `selectAll`, `moveCursorTo(offset, extend)`. `deleteTextRange` clamps and remaps anchor (`:190-199,211-215`). Byte-budget admission `canInsert/canReplace` (`:217-225`).

### Grapheme handling (`src/core/input/text_boundaries.zig`)
`nextCharacterEnd` groups a base display unit plus following zero-width continuations (`cell_width == 0`) via `display_width.displayUnitAt` (`:4-17`) — tests prove combining accents, flag pairs, skin-tone modifiers, and ZWJ families move as one unit (`:184-199`). `previousCharacterStart` is a **forward scan from 0** (`:19-32`) — O(n), fine for a composer. Word chars = ASCII alnum + `_` + everything ≥ U+00C0 (`:71-77`). Logical lines are `\n`-delimited (`:79-89`); paragraphs are blank-line-delimited blocks (`:104-147`). UTF-8 scalar admission is atomic — partial sequences buffered per owner, invalid dropped (`src/core/input/text_scalar.zig:70,146-180`).

### Operation modules (facade: `src/core/input/runtime.zig:33-178`)
`Runtime` owns edit_state, picker, composer_history, paste, entities, text_scalar, gestures, kill_ring, edit_history, vertical_navigation, and hands out borrow-view structs per operation (insertionState/deletionState/undoState/killRingState/…). Every mutation follows the same shape — e.g. `composer_insertion.zig` `insertSlice` (`:90-118`): prepare an undo delta → discard pending auto-separator → reset vertical nav → apply picker policy → convert any skill token being typed into → insert → shift entity spans (`registered_entities.shiftForInsert`) → reconcile inline picker → commit undo entry. Bounded variant checks the **expanded** length (paste placeholders count at full text size — `insertSliceBounded` via `expandedInputLen`, `composer_insertion.zig:30-66`).

### Registered entities (`src/core/input/registered_entities.zig`)
Three atomic in-text entity kinds: **paste placeholders, image tokens, skill tokens** (`Kind`, `:21-25`; `SkillTokenSpan` `:12-19`). The state owns paste text + skill strings (`:50-57`). Queries (`entityStartingAt/EndingAt/Containing/Overlapping`, `:170-310`) make entities atomic for cursor motion and deletion (`atomicForwardDeleteEnd` `:603`); spans are shifted on insert and adjusted on delete (`:348,408`). A "pending auto separator" implements the trailing space after file/skill completion that the next typed space claims (`:567`).

### Undo/redo
- Storage `src/core/input/edit_history.zig`: delta `Entry{start, removed, inserted, cursor_before, cursor_after}` (`:8-43`); undo/redo stacks capped at **100 entries / 1MB retained bytes** (`:5-6,98-116`); two-phase `prepare` → `commit`; oversized edits become a `boundary` that wipes both stacks (`:87`).
- Application `src/core/input/composer_undo.zig`: `plan()` validates the recorded bytes still match at `entry.start` and refuse to overlap entities (`:24-46`); on mismatch the whole history is dropped with a trace (`:58-102`). Coalescing: none — every insert is its own entry [추정: felt granularity comes from per-keystroke entries; no timer coalescing found].

### Prompt history (`src/core/input/composer_history.zig`)
Full-fidelity snapshots per submitted prompt: text + pasted blocks + images + image/skill token spans (`EntryView` `:23-29`; `record` dedupes against the last entry and caps at `max_entries`, `:369-401`). `navigate(delta)` (`:445-540`): entering history **captures the current draft** as a snapshot; navigating past the newest entry restores the draft; recall is limit-checked via expanded length and renumbers paste ids (`nextPasteIdAfter`). Bound to C-p/C-n (`shortcuts.zig:17-18`) and up/down at composer edges (`Action.history_up/down`, `input_action.zig:73-74`).

### Kill ring (`src/core/input/kill_ring.zig`, `composer_kill_ring.zig`)
Kill kinds: `whitespace_word_left` (C-w), `line_start` (C-u), `line_end` (C-k) (`composer_kill_ring.zig:14-18`). The ring is a **single structured slot**, not a rotating ring: each kill replaces the previous (`kill_ring.zig:113-116`), and the payload preserves text **plus image attachments and image/skill token spans** (`State` `:45-51`, `deleteRange` captures then removes `:86-117`). `yank` (C-y) re-inserts and re-registers entities at shifted offsets, replacing the selection if any (`:119-200`).

### Multiline input
Three entry paths to a newline: Shift/Alt+Enter via kitty (`escape_parser.zig:112-114`), `ESC CR` (`:407-412`), C-j (`shortcuts.zig:27`). Plus **backslash continuation**: pressing Enter with `\` immediately before the cursor replaces it with `\n` as a single undoable edit (`src/core/input/composer_line_continuation.zig:27-51`).

### Soft wrap / visual layout (`src/ui/input/visual_layout.zig`, 1367 lines)
Pure function of `(input, cursor, terminal_cols, entities…)` → an iterator of `Unit` and `Row` events (`Source` `:51-64`, `Event` `:103-113`). Units: text, tab, `paste_placeholder`, `skill_token`, `image_badge` (`UnitKind` `:16-24`) — entities render atomically. Break kinds: `hard_newline | soft_wrap | input_end` (`:11`). Wrap policy: **spaces never wrap — they hang past the right margin and painters clip them**, so continuation rows start at the word (`:146-148` doc comment); **word-aware wrap** — a word that doesn't fit the remaining cells but would fit a fresh row wraps whole; words wider than a row fall back to per-character split (`:278-282`). Derived queries: `summarize` (total rows + cursor point + optional anchor point, `:400`), `cursorPointAt` (soft-wrap boundary belongs to the following row, `:432-437`), vertical scans with **preferred column** semantics (`scanAdjacentRow`/`scanRowDelta` `:549-698`; the sticky column lives in `src/core/input/vertical_navigation.zig:32-56` and targets are vetoed inside entities `:122` test). `visibleWindow(cursor_row, total, limit)` (`:699`) scrolls the composer window.

### Size growth
`inputRowLimit(content_bottom) = content_bottom/2 + 1` — the composer may take up to **half the content area plus one row** (`src/ui/footer/input_presentation.zig:201-205`); `cappedInputRows` derives `input_extra` = extra footer rows beyond the base one (`:206-220`); `measureRawInputGeometry` computes summary + window + picker anchor column per frame (`:239-296`). Byte limits: composer 8 MiB, decision prompts 4 KiB (`src/core/input/paste_framing.zig:16-35` `InputLimits`). Limit rejections flash via `input_limit_rejection` state (`src/core/input/input_limit_rejection.zig:4-23`).

---

## 3. Paste — framing and pasted blocks

### Framing (`src/core/input/paste_framing.zig`)
On `.paste_start` Core calls `paste.begin(owner, limit)`; owners: `composer | decision_prompt | question_freeform | approval_amendment` (`Owner` `:8-14`) — decision prompts *count and discard* bytes instead of buffering (`:92-157` `consumeByte`). The composer filter accepts CR/LF/Tab/printables (≥0x20 incl UTF-8) (`:112-135`); the end marker `\x1b[201~` (`:6`) is matched incrementally, and because ESC snapshots the buffer length (`end_candidate_buffer_len`, `:118-121,146-152`), marker bytes never leak into the payload. Boundary states `capturing/end_candidate/unsafe` and `settleDeliveryEpoch` decide finish vs reject (`:158-166`); overflow beyond the buffer limit is counted, not stored (`:128-134`).

### Finalization (`src/core/app/input_paste_runtime.zig`)
`settleTerminalPasteDeliveryEpochWithLimits` dispatches by owner (`:96` [call chain 추정], composer branch `:204-214`): `finalizePastedBlock` (`:248-320`):
- image-path token → pending image attachments (`:251-256`);
- `countCodepoints(text) <= 1000` → plain inline insert (`pasted_blocks.zig:53-56` `shouldUsePlaceholder`, threshold `large_paste_char_threshold = 1000` at `:7`);
- else: allocate a `PastedBlock{id, text, line_count}` and insert the placeholder **`[Pasted text #N, M lines]`** (`formatPlaceholder`, `pasted_blocks.zig:59-63`), register its span as an entity (`input_paste_runtime.zig:306-315`), bump `next_paste_id`, and set an undo **history boundary** (`:316-318`).

### Round trip
On submit, placeholders expand back to full text: `expand`/`expandRange` (`pasted_blocks.zig:72-138`); `expandedLen` (`:169`) is what all input-limit math uses. In the visual layout the placeholder is one atomic clipped unit (`visual_layout.zig` UnitKind `.paste_placeholder`; painter clip in `src/ui/render.zig:500-508`). Deleting any part of the placeholder removes the whole block (entity atomicity, §2).

---

## 4. Slash commands

### Registration
Production table `src/builtins/commands.zig:414-457` — 40 `SlashSpec` entries (help/clear/new/reset/resume/continue/rename/login/logout/setup/stats/usage(/cost)/status/background(+stop/open/logs subcommands as separate payload specs `:428-430`)/image(/img)/images/model/models/permissions/allowlist/undo/mcp/skills/copy/feedback/trace/compact/settings/alias/credits(/balance)/paste/fast/statusline/sound/workspace/version/quit(/exit)); `slash_registry` at `:457`. Spec shape `src/core/slash_commands/command_specs.zig:160-172`: command, aliases, help_entry, completion_description, presentation_category (11 categories, `:130-158`), `has_args`, `accepts_payload`.

### Dispatch (`src/core/slash_commands/command_router.zig`)
`parse(registry, cmd)` walks specs; payload commands match by prefix (`matchedSlashPrefix`), exact commands by equality, producing a `ParsedCommand` union with the trimmed payload (`:7-49,100-157`). `route()` calls into a `CommandHandlers` **vtable of function pointers with `ctx: *anyopaque`** (`:51-143,158+`) — the app supplies handlers; the router is UI-free and fully unit-tested.

### Completion & menu UX
- Trigger detection is editor-derived, not modal: `picker_state.zig` `findInlineSlashQuery` / `inlinePickerTriggerKind` (`:118-136`) — a leading `/` in the trimmed input arms the slash picker; a per-kind **dismissal memory** (Esc) survives until the trigger kind changes (`:83-96`).
- Matching ranks exact-command prefix > alias prefix > substring (`command_specs.zig:462-499`; behavior pinned by tests `:1703-1725`). `slashCompletionHasArgs` + per-command **argument completion tables** for `/statusline`, `/sound`, `/permissions`, `/workspace`, and a large scoped `/allowlist` tree (`:686-815`); `argCompletionAnchor` (`:580`) anchors the picker column under the argument token.
- The menu itself mixes commands with **skills as `/skill` entries** (`src/ui/footer/picker_presentation.zig` `mixedSlashCompletionCount/…IsSkill/…Text` `:690-717`), computes column widths (`:824+`), and renders header + option rows (`:749,907,949`); layout `slashMenuLayout` (`:396-470`). Selection/window state lives in `picker_state.zig` (`slash_completion_index/window_start`, `:56-58`); Tab completes inline (`appendInlineCompletionSuffix` shows the ghost suffix, `input_presentation.zig:709`).
- `/statusline`, `/usage`, `/workspace` open a **compact command menu** in the footer (toggle list / usage report / workspace list) — `src/ui/footer/compact_command_menu_presentation.zig:29-55` (statusline = title + blank + one row per toggle choice; workspace pins 5 rows).
- `/help` opens a filterable catalog menu grouped by category (`command_specs.zig` `HelpMenu` `:195-236`, catalog queries search all metadata `:382-399`).

---

## 5. Pickers and menus

### Generic inline picker model (`src/core/input/picker_state.zig`)
One state struct holds per-kind completion index + window start + dismissal (`:55-71`). Four inline kinds (`InlinePickerKind:13-18`):
- **slash** — `/` prefix (above);
- **model** — `/model ` arms a **3-stage flow: model → effort → fast** (`ModelPickerStage:7-11`, `beginModelPickerFlow:192`, fast options `["normal","fast"]` `:20`), pending model buffered in state;
- **file** — `@` trigger; `FilePickerQuery` records `@` offset and supports the quoted form `@"` that stays open across spaces (`:28-37`); terminator set `isFilePickerTerminator` (`:257`);
- **skill** — `$` trigger (`InlineSkillQuery:39-46`), binding produces a skill token entity (`registered_entities.bindSkillToken:466`).
Every edit reconciles the picker (index reset; dismissal cleared when the trigger kind changes — `reconcileInlinePickerAfterEdit:91-96`). Prefix filtering helper `filterCompletionLabels` (`:244`).

### Windowing/scrolling
`picker_presentation.zig`: `pickerWindow` centers selection; `edgeScrollPickerWindow*` scroll only at edges (`:378-393`); default max rows from `list_window` (`input_presentation.zig:23`); reserved-row math vs terminal height (`:363-376`).

### Menus (footer "projections", `src/ui/footer/render_input.zig`)
Each modal menu is a read-only projection + a presentation module that owns row math:
| Menu | Projection (render_input.zig) | Presentation | Data shown |
|---|---|---|---|
| Model (`/models`) | `ModelMenuProjection:45-66` (query filter, provider filter, load_state) | `model_menu_presentation.zig` layout header+gap+items (`:17-75`) | model list from model cache; effort/fast stages |
| Resume (`/resume`) | `SessionMenuProjection:68-96` incl. **load-more row** (`isLoadMoreIndex:89`) | `resume_menu_presentation.zig:182-190` | session summaries |
| Help (`/help`) | `HelpMenuProjection:98-112` | `help_menu_presentation.zig:40-52` | filtered slash catalog by category |
| Settings (`/settings`) | `SettingsMenuProjection:114-130` | `settings_menu_presentation.zig:45-57` | settings_catalog items (incl. statusline toggles, `src/core/config/settings_catalog.zig:53-55`) |
| Skills (`/skills`, `$`) | `SkillsMenuProjection:36-43` | `skills_menu_presentation.zig` `PreparedSkillsMenu:84-150` | skills with source labels |
| Compact (statusline/usage/workspace) | `CompactCommandMenuProjection:195-199` | `compact_command_menu_presentation.zig:29-55` | toggles / usage / workspace dirs |

Keys are uniform: ↑↓ move, Enter select, Esc dismiss; each menu has its own hint row composer (`input_presentation.zig:415-533`). Navigation math is shared (`visibleNavigationItemsForBudget` per menu).

---

## 6. Approval UI and Question UI

### Approval prompt
- **State/semantics** `src/core/permissions/approval_decision.zig`: `Action = deny|submit|tab|backspace|number|insert_ascii|move_choice|edit_draft` (`:33-42`); `DraftAction` cursor/delete ops for the amendment editor (`:18-31`); `choice_index` → `ToolPermissionDecision` (once/always/deny); `confirmation_only` mode collapses to Confirm/Cancel (`:77,84-87`).
- **Keys**: 1–3 direct pick, ↑↓/Tab cycle, Enter confirm, Esc cancel, Ctrl-C deny (`src/ui/input/runtime.zig:65-77`); printable bytes type into the **amendment** draft; Tab enters amendment (hint `interaction_state.zig:20`). Amendment paste is a first-class paste owner (`paste_framing.zig:13`).
- **Rendering** (`src/ui/footer/approval_ui.zig`): inline panel 8 rows compact / 11 spacious (spacious ≥ 34 terminal rows) + 3 chrome rows (`src/ui/footer/interaction_state.zig:12-15`); header "Permission needed · Choose one" with a right-aligned kind tag and subagent origin (`composeApprovalHeaderRow:1860+` [추정 exact line ~1856]); the wrapped command target uses one shared `CommandSegmentIterator` for both measurement and painting **so the row count cannot drift from painted rows** (`:268-294`, doc `:208-211`); choice rows `:1524-1541`; hint picks the widest variant that fits (`approvalHint:1739-1752`, variants `interaction_state.zig:19-26`).
- **"Always" scoping wording** `approvalAlwaysChoice` (`approval_ui.zig:1986-1992`): MCP tool → "2. Allow this MCP tool for this session"; `terminal.exec` → "2. Yes, and don't ask again for this exact command"; default → "2. Yes, and don't ask again for this request" (constant `interaction_state.zig:17`). Amendment reword: "1. Yes, and tell fx what to do next" / "3. No, and tell fx what to do differently" (`:1715-1721`).
- **Readiness — the anti-blind-approve gate** (`src/ui/footer/approval_readiness.zig`): file approvals (diff review) only accept the affirmative once (a) renders settled for this approval, resize idle, layout non-zero (`settledFileApproval:15-39`), and (b) the committed screen frame at the same request id + dimensions actually showed the file identity, all decision controls, and the changed/notice rows (`screenCommitSupportsAffirmative:65-75`; commit record in `interaction_state.zig` `ApprovalScreenCommit:28-43`). File-approval layout: `projectFileApproval` (`approval_ui.zig:468`), desired rows (`:454`), scrollable document (`ApprovalScreenState.scrollDocument`, `interaction_state.zig:48-59`).

### Question prompt
- **State** `src/core/agent/question_prompt.zig`: batch of entries, each with options + an always-appended synthetic **"Other"** freeform slot the model never sees (`freeform_option_label:57`, `OwnedQuestionEntry:65-88` with its own freeform buffer/cursor/preferred column). `Action = cancel|submit|next_entry|backspace|select_ordinal|insert_ascii|move_choice|edit_freeform` (`:30-39`); events include `all_decided` when every entry has an answer (`:41-51`).
- **Keys**: 1–9 ordinal, Tab next entry, Enter submit, Ctrl-C cancel, shift+↑↓ moves choice; when the freeform slot is selected, digits/printables insert and emacs bindings edit (`src/ui/input/runtime.zig:139-215`).
- **Rendering** `src/ui/footer/question_ui.zig` `composeQuestionPanelText` (`:40-66`): bold wrapped question, options as `N) label` — **selection reads purely from white-vs-gray contrast, no caret** (comment near `:105`); two-column label+description wrapping (`:120+`); freeform inline editor rows via `src/ui/footer/question_freeform_layout.zig` (shared indent `option_row_indent:25`, UTF-8-safe cursor snap `normalizedCursor:37-45`, wrap `nextLine:46+`). After the batch resolves, a transcript block shows each question with its muted answer, or `■ Cancelled` (`composeQuestionResolutions:415-446`).

---

## 7. Footer frame — layout and invalidation

### Row geometry (`src/ui/footer/viewport.zig`)
`Geometry` (`:13-30`): `top` (outer top of the fx-owned band — every paint clears from here down), `top_divider`, `input_base`, `input_first`/`input_window_first` (committed composer window), `bottom_divider`, `hint`, optional `activity_row`. Mouse clicks map back to composer offsets via `inputPointerPosition` (`:31-50`).

### Painting model
`FooterViewport` composes the band into a cell grid and emits **the minimal ANSI delta against the shell's shadow grid — "the shadow is the single source of truth for what is on the terminal; there is no private prev-row state"** (doc `:90-95`); `invalidateAfterExternalClear` forces a full band repaint (`:155-158`); `paintFooterIntoSurface` (`:220`).

### Frame planning (`src/ui/footer/paint_plan.zig`)
`FooterPlannerInput` (`:60-97`) carries everything: render ctx, approval projection, input summary/window/extra rows, picker rows/kind/items, banner and gap state, transcript cursor state. `composeFooterFrame` (`:720+`) assembles rows in order: queued-prompt banner → (top divider when composer hidden) → **input rows** (with selection range + inline completion ghost) → picker divider → exactly one of {approval panel | question panel | compact command menu | picker rows} → hint row. File approvals and the transcript viewer branch to dedicated frames (`:735-742`). `FooterFramePlan` also resolves activity placement and bottom reservations (`:99-110`).

### Invalidation (`src/ui/footer/surface_invalidation.zig`)
Footer height changes (`footer_extra`/reserved base rows) are detected (`FooterExtraUpdate.changesFooterExtra:20-24`, `detectFooterExtraChange:39-45`), logged, and turned into `FrameInvalidationRange`s appended to the paint plan (`appendVisibleFooterInvalidation:110-131`) with reasons `external_clear` (shrink) vs `reserved_gap_clear` (grow) (`:100-108`); off-screen or non-intersecting invalidations are skipped with a trace. The measure→prepare→commit pipeline lives in `src/ui/footer/surface_frame.zig` (`SurfaceFooterMeasurement:676-737`, `commitSurfaceFooterFrame:1032`, retarget on plan change `:654`).

### Input row composition
`composeVisibleInputRows` (`input_presentation.zig:629-685`) walks the visual-layout iterator, emitting only rows inside the window; a queued (unsent) prompt is re-rendered with composer chrome as a banner card (`composeQueuedPromptCard:688-706`).

---

## 8. Status line (the hint row)

Composed in `src/ui/render.zig` `buildHintLine` (`:391-460`); segments joined by `" · "` (`appendStatusSegment:254-266`), left to right:
1. `run /login` when no credential (`:416-418`)
2. `queued N` (`:419-422`)
3. **permission mode** `ask`/`auto`/`YOLO` (auto/YOLO styled, `permissionModeStatusLabel:246-251`), shown only if it fits with the model label (`leadingPermissionModeFits:270-280`)
4. **compact model label** — strips `provider/` and `claude-` prefixes → `opus 4.7` (`compactModelLabel:219-244`)
5. **effort label** if non-default and supported; **`⚡︎`** if fast mode on a fast-capable model (`:425-436`)
6. session title (≤ 32 cells, `:437-439`, cap `:217`)
7. `Context: {used}k/{total}k {pct}%` (`:441-454`)
8. **workspace identity** = workspace label + `(git branch)` with budgeted clipping — branch gets at most half the identity budget, prefix-clipped with `…` (`StatuslineItems:207-213`, branch logic `:332-348`).
Segments 6–8 are gated by the `/statusline` toggles (`settings_catalog.zig:53-55` `statusline_context/session/workspace`, default off).

Placement/overrides: `composeHintRow` (`input_presentation.zig:296-393`) — the left text is overridden in priority order by question hint → `press ctrl+c again to exit` → selected-subagent `label · status · hint`; the right-aligned tag is `esc again to clear` → danger status (red, suppressed during transient interactions, `dangerStatusText:394-413`) → upgrade status (dim).

---

## 9. Minimum viable slice for the xfx port

Ranked by "what makes it feel like fx first", with honest deferrals:

**Tier 1 — the editor loop (do first, ~feels 60% right)**
1. Byte decoder subset: bare-ESC timeout disambiguation + CSI arrows/Home/End/Del/PgUp-Dn + modified arrows + bracketed paste 200/201 + the emacs control-byte table (`shortcuts.zig:11-30` is 20 lines — port verbatim). Keep fx's invariant: one byte → ≤1 event, escape-then-replay ordering (`terminal_action_decoder.zig:105-114`).
2. Editor core: flat `String` + byte cursor + selection + text_boundaries port (grapheme motion via `unicode-segmentation`, width via `unicode-width` — strictly better than fx's homegrown display-unit walk).
3. Soft-wrap visual layout with fx's two wrap rules (hanging spaces `visual_layout.zig:146-148`, word-fit wrap `:278-282`), sticky-column ↑↓, and the growth cap `content_bottom/2 + 1` (`input_presentation.zig:201`).
4. Hint/status row: permission mode + compact model label + context% (`buildHintLine` ordering) — cheap, high identity value.

**Tier 2 — the product surfaces**
5. Slash registry + router (the `SlashSpec` table + prefix-ranked completion + vtable router ports 1:1 to a Rust trait object or enum dispatch) and the inline slash picker with dismissal memory.
6. Prompt history: text-only snapshots + draft capture on entry (`composer_history.zig` semantics; defer images/entities in snapshots).
7. Approval prompt: 3-choice inline panel, 1-3/↑↓/Tab/Enter/Esc/Ctrl-C, the three "always" wordings (`approval_ui.zig:1986-1992`). **Defer** the readiness commit-gate and amendments — they are correctness hardening, not feel.
8. Gestures: Ctrl-C-twice-to-exit (3s) + double-Esc-to-clear (500ms) (`gesture_state.zig:3-4`) — tiny and very "fx".

**Tier 3 — paste + undo**
9. Bracketed-paste framing with end-marker trimming + the 1000-codepoint pasted-block placeholder (`[Pasted text #N, M lines]`) with expand-on-submit; treat the placeholder as an atomic entity for cursor/delete.
10. Delta undo stack (100 entries/1MB caps) + single-slot kill ring, text-only.

**Defer (low feel-per-line)**: full kitty CSI-u matrix (keep escape/enter/backspace disambiguation only), mouse (wheel-scroll only, or nothing), skill `$` tokens + image tokens/badges, subagent input routing, file-approval diff screen + readiness gate, question freeform inline editor (ship ordinal-only questions first), compact command menus, model 3-stage picker (plain `/model <id>` first), theme monitor / cursor probe / native clear probe, queued-prompt banner cards, shadow-grid minimal-delta painting (a full footer-band repaint per frame is fine at these sizes at first).

**Rust crate notes (honest)**
- `crossterm`: `EnableBracketedPaste`, `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)`, mouse capture, and its event parser replace ~80% of `escape_parser.zig` + `terminal_action_decoder.zig`. Cost: you lose fx's byte-level `cancel_pending` snapshot and escape-replay ordering; acceptable for v1. If byte-fidelity matters later, `vte` or a direct port of the fx stage machine (~450 lines of logic) is straightforward since it's already pure.
- `unicode-segmentation` + `unicode-width`: grapheme clusters and cell widths; fx's zero-width-continuation heuristic becomes unnecessary.
- Rendering: fx's footer is a bespoke cell-grid diff, not a ratatui-style immediate-mode UI. For xfx either (a) ratatui with a fixed bottom `Layout` chunk — fastest, loses the transcript-scrollback cohabitation model, or (b) keep xfx's raw stdout and port `viewport.zig`'s "compose band → diff → ANSI" (recommended if the transcript stays plain scrollback like fx: the footer must own only its band and repaint from `geometry.top` down).
- `arboard` for `/copy`//`paste` clipboard.

## Tensions / gaps
- Mouse *enable* sequence: only the disable (`?1000l?1006l` on alternate-screen leave, `terminal.zig:13`) was read; the exact enable site is [추정] in the alternate-screen enter path.
- `src/ui/input/runtime.zig` is 5016 lines but ~80% tests; the production surface is the `Runtime` struct (`:846-925`) + the byte-mapping helpers (`:65-360`) — do not budget a port by file size.
- Undo has no coalescing timer anywhere I read; per-keystroke entries within 100-entry/1MB caps appear to be the intended granularity [추정].
