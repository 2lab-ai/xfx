# fx TUI core — 소스 분석 노트 (Rust 포트 PRD evidence base)

- Upstream: vercel-labs/fx @ ef1d0d0, 로컬 클론 `scratchpad/fx-src/` (이하 모든 `file:line`은 그 루트 기준)
- 스코프: `src/ui/` 렌더링/런타임 서브시스템 + `src/core/app/` UI 배선
- 표기: 근거 없는 추론은 `[추정]`

---

## 0. 한 문단 요약 (하이레벨)

fx의 메인 UI는 **alt screen이 아니라 normal 버퍼의 하단 밴드**를 소유한다. 완료된 transcript 행은 터미널 문서(=네이티브 스크롤백)로 "append"되어 영원히 남고, fx는 화면 아래쪽의 자기 소유 밴드(transcript 뷰포트 + activity 행 + footer/컴포저)만 매 프레임 다시 그린다. 프레임 커밋은 **in-process VT 에뮬레이터(shadow grid)** 와의 셀 단위 diff로 최소 바이트를 만들고, 전체를 synchronized output(mode 2026)으로 감싼다. 이벤트 루프는 단일 스레드 `poll(2)` 8ms 틱이고, 모든 비-stdin 입력(에이전트 이벤트, 리사이즈, 테마 알림, 애니메이션 데드라인)은 `collect_facts` 단계에서 폴링된다. 모달(approval/카탈로그/full transcript/서브에이전트/터미널 세션)만 alt screen을 배타 소유한다.

---

## 1. Terminal model

### 1.1 Alt screen이 아니라 main scrollback에 그린다
- Alt screen은 **소유자 enum이 있는 예외 상태**다: `AlternateScreenOwner = { none, file_approval, full_transcript, catalog_menu, subagent_manager, terminal_session }` — `src/ui/shell_runtime.zig:57-64`. 평상시 `none`.
- alt screen 진입은 모달 전용: `alternate_screen_enter = "\x1b[?1049h"` `src/core/app/app_lifecycle.zig:42`, 진입 함수 `enterAlternateScreen` `app_lifecycle.zig:996-1008` (이미 소유자 있으면 `error.AlternateScreenAlreadyOwned`).
- normal 종료 restore 시퀀스에 `1049l`이 **없다**(메인 서피스가 alt screen에 있던 적이 없으므로): `normal_exit_restore_prefix` `app_lifecycle.zig:39-41`. 반면 비정상 종료(abnormal) restore에는 방어적으로 `1049l` 포함: `app_lifecycle.zig:36-38`.

### 1.2 Raw mode 라이프사이클
- 진입: `bootstrapInteractiveApp` `app_lifecycle.zig:447-530` — `ensureInteractive`(isatty 검사, `shell_runtime.zig:96-101`) → `captureOriginalTermios`(`:103-106`) → `enableRawMode`(`:108-138`: BRKINT/ICRNL/INPCK/ISTRIP/IXON/IXOFF off, CS8, ECHO/ICANON/IEXTEN/**ISIG** off, VMIN=1 VTIME=0) → `installResizeSignal`(SIGWINCH, `:148-161`) → `installAbnormalExitHandlers`(SIGTERM/SIGHUP, `app_lifecycle.zig:81-101`; ISIG off라 SIGINT는 터미널이 안 만들므로 제외 — 주석 `:78-80`).
- 종료: `shutdownInteractiveShell` `app_lifecycle.zig:578-593` — alt screen 정리 → 시그널 핸들러 원복 → restore 시퀀스 write → `disableRawMode`(`shell_runtime.zig:140-146`, `.FLUSH`로 원래 termios 복원) → 커서를 footer top으로 옮기고 `\x1b[J\x1b[?25h\n` (`emitShutdownCleanupAndResume` `app_lifecycle.zig:1056-1067`) — **transcript는 셸 스크롤백에 그대로 남는다.**
- Ctrl-Z: `suspendToJobControl` `app_lifecycle.zig:609-620` — cooked 복원 → `raise(SIGTSTP)` → SIGCONT 후 termios 재캡처 + raw 재진입 + 레이아웃 재조회 + full repaint 요청(`resumeTerminalAfterJobControl` `:646-656`).
- 비정상 종료: 시그널 핸들러가 async-signal-safe하게 컴파일타임 상수 restore 문자열 하나를 `write(2)` (`abnormalExitHandlerWithRestore` `app_lifecycle.zig:53-68`).

### 1.3 스크롤백 보존 메커니즘 ("closer to a Unix shell")
(참고: 이 문구 자체는 repo 내 문서에서 미발견 — 외부 마케팅 클레임 `[추정]`. 메커니즘은 코드로 확인됨.)
1. **런치 시**: `queryCursorPosition`(CSI `6n`, 100ms 데드라인, `shell_runtime.zig:178-206`)으로 현재 셸 커서 행을 알아낸 뒤, 기존 셸 출력 위 행들을 커서를 최하단으로 옮기고 `\n`을 반복 출력해서 스크롤백으로 밀어 넣는다: `pushLaunchRowsIntoScrollback` `app_lifecycle.zig:556-583`, 정책 `prepareStartupViewport` `:531-554` (`startup_scrollback` 설정 게이트).
2. **세션 중**: 화면을 벗어나는 완료된 transcript 행은 "document append"로 normal 버퍼에 기록된다 — CR-before-LF 정규화된 wire 바이트(불변식: `frame_scroll_plan.zig:8-12`; 준비: `prepareTranscriptDocumentAppendBytes` `src/ui/transcript/painter.zig:2720-2752`)를 transcript 끝 지점에 autowrap 켜고 출력(`appendDocumentMovement` `src/ui/render_engine/terminal_diff.zig:1348-1397`). 스크롤 자체는 **CUP 최하단 + 리터럴 `\n` 반복**으로 일으킨다(`writeTerminalScroll` `terminal_diff.zig` — `\x1b[{bottom};1H` + `'\n'`×rows). 터미널이 진짜로 스크롤하므로 위로 사라진 행이 네이티브 스크롤백에 들어간다.
3. **마우스 리포팅을 메인 서피스에서 켜지 않아서** 사용자의 스크롤휠이 터미널 네이티브 스크롤백으로 동작한다 — 테스트가 이를 계약으로 고정: "interactive mode leaves native terminal scrollback enabled" `src/ui/terminal/terminal.zig:135-142` (1000h/1002h/1006h 부재 검증).
4. tmux 예외: 리사이즈 리셋 시 `tmux clear-history` CLI로 pane 히스토리를 지운다 `shell_runtime.zig:208-220, 315-325`; Apple Terminal(비-tmux)만 히스토리 리셋에 RIS(`\x1bc`) 사용 `shell_runtime.zig:357-362, 522-526` + `composeWireFrame`의 RIS 후 모드 재설정 `terminal_diff.zig:589-593`.

### 1.4 이스케이프 프로토콜 인벤토리
| 프로토콜 | 시퀀스 | 근거 |
|---|---|---|
| kitty keyboard push (flag 1 = disambiguate) | `\x1b[>1u`, pop `\x1b[<u` | `terminal.zig:4`, `app_lifecycle.zig:37,40` — **tmux 아래서는 생략** `terminal.zig:5,29-34,122-133` |
| xterm modifyOtherKeys=2 | `\x1b[>4;2m` / 해제 `\x1b[>4;0m` | `terminal.zig:4`, `app_lifecycle.zig:37-41` |
| bracketed paste | `\x1b[?2004h` | `terminal.zig:4` |
| autowrap OFF (소유 밴드 페인트용) | `\x1b[?7l`, append 시 일시 `?7h` | `terminal.zig:4`, `terminal_diff.zig:1388-1391` |
| synchronized output | `\x1b[?2026h/l` 프레임 감쌈 | `terminal_diff.zig:595,628` (composeWireFrame); TERM=dumb/`FX_SYNC_UPDATES`로 게이트 `shell_runtime.zig:344-355,499-520` |
| SGR mouse (alt screen 전용) | `\x1b[?1000h\x1b[?1006h` | `app_lifecycle.zig:43`, `setAlternateScreenMouseTracking` `:1026-1055` |
| OSC 2 title | `\x1b]2;fx · …\x07` | `src/ui/render.zig:687,736` |
| OSC 11 배경색 질의 | `\x1b]11;?\x1b\\` + DA1 fence `\x1b[c` | `terminal.zig:8-11` |
| 테마 변경 알림 mode 2031 | `\x1b[?2031h/l` | `terminal.zig:6-7` |
| color-scheme DSR | `\x1b[?996n`, 응답 `\x1b[?997;1n/;2n` | `terminal.zig:8`, `theme_monitor.zig:288-289` |
| 리사이즈 커서 프로브 | `\x1b[?2026h\x1b7\x1b[1G\x1b[?6n\x1b[2G\x1b[?6n\x1b8\x1b[?2026l` (col1/col2 이중 DSR로 프로브 응답 fingerprint) | `src/ui/terminal/cursor_probe.zig:384-385`, 페어 판별 규칙 주석 `:17-21` |
| OSC 8 hyperlink | surface가 intern, diff가 열고 닫음 | `frame_surface.zig:23-29,216`, `engine.zig diffBand`의 hyperlink transition |

## 2. Frame model

- **프레임 = 셀 그리드**다 (라인 배열이 아님). `FrameSurface`: `FrameCell` 배열 + hyperlink/combining-suffix intern 풀, shadow grid로부터 초기화 — `src/ui/render_engine/frame_surface.zig:11-29,116-190`. 셀마다 소유자(`CellOwner`) 정책 검사(`writeCell` `:236`).
- **레이아웃**: `types.Layout`(rows/cols/content_bottom/divider/input/hint) — `terminal.zig:47-59`; `frame_layout.solve`가 transcript_area/footer_area/activity 배치를 가진 `FrameLayout` 계산 — `src/ui/render_engine/frame_layout.zig:156-300`; footer 내부 행 배치는 `footer_layout.resolve` — `src/ui/render_engine/footer_layout.zig:3-39`. footer 높이↔transcript 점유가 상호 의존이라 **fixed-point 반복**으로 후보 레이아웃을 수렴시킨다 — `frame_fixed_point` 사용처 `src/core/app/app_render_runtime.zig:3294-3420` (`prepareCandidate`/`resolveCandidate`).
- **Shadow VT**: in-process 경계형 VT 에뮬레이터 `Grid` — `src/core/terminal/engine.zig:1-3` ("Bounded text-terminal engine … deterministic rendering tests"), `Grid` `:222`, `feed` `:489`. "load-bearing: frame commit diffs the target surface against it" — `app_lifecycle.zig:490-492` (`enableShadowVt`). **터미널에 쓰는 모든 바이트를 shadow에도 feed** — `writeLifecycleTerminalBytes` `app_lifecycle.zig:1069-1080`.
- **커밋 파이프라인** `buildAndFlushFrame` — `src/ui/render_engine/frame_builder.zig:70-134`:
  1. `prepareFramePlan`(`:171-194`): plan 검증 + resize/terminal_scroll/owned-band 변경 시 invalidation 추가.
  2. `prepareTerminalMovementForFrame`(`terminal_diff.zig:1213-1300`): terminal transition(alt→normal 복귀) + document append + alignment scroll 바이트를 만들고, 그 바이트를 shadow 클론에 feed해서 `post_movement` grid를 얻는다(스크롤 후 상태 예측).
  3. `FrameSurface.initFromShadow` → body/footer/activity **painter 콜백**이 surface에 칠함(`paintFrameSurface` `:220-259`; transcript는 `.retain`으로 이전 프레임 밴드를 grid에서 복사만 할 수도 있음 — `frame_retention`).
  4. `terminal_diff.flushFrame`으로 커밋.
- **terminal_diff.flushFrame** — `terminal_diff.zig:335-560`:
  - surface→target Grid 복사, `countChangedCells`(post-movement shadow 대비), 변화 0 + 커서 일치면 **no-op skip**(`skipped_noop` `:399-416`).
  - `composeWireFrame`(`:580-631`): `[?2026h` → `[?25l` → (reset 시 RIS/`2J 3J H`) → movement 바이트 → invalidation range별 `Grid.diffBand` → CUP 커서 → `[?2026l` → `[?25h`.
  - `Grid.diffBand` — `engine.zig:2191-…`: 행마다 변경 span(first..last)만 찾아 CUP 후 재출력, SGR은 상태 추적으로 최소 전이만(`emitSgrTransition`), OSC 8 전이, wide-glyph 겹침 시 `\x1b[{n}X` 선지움, tmux hyperlink wrap 보정.
  - 커밋 후 **self-check**: 방금 쓴 wire 바이트를 shadow 클론에 feed해 target과 대조(`targetMatches`), 불일치/부분 write면 커밋 실패 처리 + retry invalidation 기록(`recoverPartialFrame` `:633-…`, `record_retry_invalidation` `frame_builder.zig:340-343`). 성공 시 fed shadow가 새 authoritative shadow가 된다(`:520-540`).
- **Paint plan**: `PaintPlan` = layout + transcript/footer/activity band + invalidation set(최대 8 range, reason enum 포함) + cursor target + `synchronized_update` — `src/ui/render_engine/paint_plan.zig:345-364, 98-174, 64-96`. `FrameRepaintWindow`가 diff 대상 행 범위를 결정(`:181-224`); retained transcript면 그 밴드는 diff에서 제외(`frame_builder.zig:158-168`).

## 3. Event loop

- 코어 루프 `event_loop.run` — `src/ui/event_loop.zig:76-122`: 반복마다
  `collect_facts` → (프로브가 유예한 바이트 drain) → `pollInput(timeout)` → 읽을 수 있으면 최대 **32회 × 128B** 버스트 read, 바이트 단위 `handle_byte` → `settle_delivery_epoch` → `commit_frame`. 지속 입력이 렌더를 굶기지 못하게 32회 캡(`:16`, 테스트 `:507-551`). 콜백 명세 `EventLoopCallbacks` `:6-14`.
- 타임아웃 = **8ms 고정 틱**: `active_poll_timeout_ms: i32 = 8` — `src/main.zig:183`, 루프 호출 `main.zig:950-955`. 동적 타임아웃 훅은 wasm 전용(`loopPollTimeoutMs` `main.zig:977-981`). 타이머는 별도 fd 없이 이 틱에서 milliTimestamp 데드라인 비교.
- `pollInput`은 `poll(2)` stdin 단일 fd — `shell_runtime.zig:257-278` (readable/hung_up/has_error).
- **모든 비-stdin 입력은 `collect_facts`에서 폴링** — `loopCollectFacts` `main.zig:2494-2590`:
  테마 모니터 → 업그레이드/권한/모델캐시/MCP/auth facts → **리사이즈**(SIGWINCH 핸들러는 atomic 비트만 세팅: `ResizeApprovalInterlock` `src/ui/resize_runtime.zig:46-90`; 디바운스+커서 프로브는 `collectResizeFacts` `resize_runtime.zig:183`) → escape 타임아웃 flush → `WorkerAppRuntime.tick`(**에이전트/툴 이벤트 drain**: `refreshTasks`+`drainEvents`+애니메이션 arm — `src/core/app/app_worker_runtime.zig` tick 본문) → `pacer.tick`(`main.zig:2582`; approval/question 중엔 `pacer.pause`).
- **재렌더는 요청 기반**: `RenderRequestState`가 Reason 집합(`first_frame, transcript, footer, modal, subagent_panel, animation, notification, resize, external_damage`)과 invalidation을 모은다 — `src/ui/render_request.zig:5-16, 92-…`. `loopCommitFrame`(`main.zig:2588-2595`) → `flushRequestedFrame`(`app_render_runtime.zig:1238-…`)이 `beginAttempt`(`render_request.zig:281-305`; pending 없으면 null → 커밋 스킵)로 스냅샷 뜨고, 실패/입력-펜딩 시 `restore`로 되돌린다(입력 펜딩 abort는 연속 4회 캡 `:69`).
- **애니메이션 스케줄**: 50ms 간격(`animation_interval_ms` `render_request.zig:68`), phase는 -8..31의 40프레임 사이클, blink 반주기 10프레임=500ms 벽시계와 comptime으로 동기 강제(`:64-84`). worker tick이 `requestAnimationDue`(`:246-259`)로 후보를 arm → attempt에 실려 커밋되면 `committed_animation_phase` 갱신(`:307-333`).

## 4. Transcript

- **저장**: `TranscriptRuntime`(파사드, `src/ui/transcript/runtime.zig`) + `store.zig`. 엔트리는 `TranscriptEntry` union — raw_bytes(클래스 부착), semantic notice, user turn, assistant turn/table/code-block/thematic-rule — `src/ui/render_engine/transcript_blocks.zig:284-357`. 별도로 렌더된 ANSI 바이트 캐시(`transcript`)를 유지(계약 주석 `store.zig:1-17`), 바이트 상한 하 구조적 보존(`enforceStructuredRetention` `store.zig:744`). 툴 상태는 "pinned" 엔트리로 append/replace(`appendPinnedToolStatusAtomic` `store.zig:1022`, `replacePinnedToolStatus*` `:1398-1548`).
- **렌더/래핑**: `renderEntryToBlock`/`renderEntriesToBytes` `transcript_blocks.zig:1512,2127`; assistant 텍스트는 SGR·OSC-8 상태를 추적하는 워드랩(`wrapAssistantText` `transcript_blocks.zig:413` → `assistant_wrap.zig:58`, reflow 포인트 `WordBreaks` `:22-38`); 블록 간 간격 정책 `blockGapRowsBetween` `transcript_blocks.zig:204`.
- **하이라이트**: 소형 수제 렉서(keyword/string/number/comment 4클래스, 256색 2팔레트) — `src/ui/render_engine/code_highlight.zig:8-41`, 언어 테이블 `code_highlight_languages.zig`. syntax tree 없음.
- **측정**: ANSI 스킵 + 표시폭 테이블로 visual row 계산(DECAWM-off 전제) — `src/ui/render_engine/transcript_measure.zig:12(walkText),133(visualRowsForLine)`.
- **뷰포트/선택**: hard line 인덱스에서 가시 tail 윈도우 선택(`buildViewportSelection` `src/ui/render_engine/viewport_selection.zig:29-129`); 앵커/최소 가시 행/footer 갭은 `viewport_runtime.zig`(`initViewportWithReservedRows:47`, `reanchorTop:106`, `footerTopRowForExtra:255`). painter가 `PreparedTranscriptSurfacePaint`를 만들어 surface에 칠함(`painter.zig:1234-1360, 3211`).
- **인라인 스크롤은 구현하지 않는다** — normal 버퍼 + 마우스 리포팅 off라 터미널 네이티브 스크롤백이 곧 히스토리 UI(§1.3-3). 인앱 스크롤이 필요하면 아래 full transcript로 승격.
- **full_transcript_screen**(`src/ui/full_transcript_screen.zig`): alt screen 프로젝션. `DetailDepth {review, full}`(`:27`) — 접힌 툴 출력/디테일 레코드까지 포함한 `Projection` 재구성(`buildProjection:4302`), 뷰포트 오프셋 셀렉터(`:288`), interruptible 렌더(입력 오면 중단, `:6548`). 진입 `openFullTranscript` `app_lifecycle.zig:894-917`: projection depth 전환 → alt screen 진입 → **마우스 트래킹 on**(휠 스크롤). 닫을 때 primary 화면을 exact-retain/repaint로 복원(`closeFullTranscript` `:919-946`, `fullTranscriptPrimaryRestore` `runtime.zig:6062`).

## 5. Streaming UX

- **Pacer** `AssistantPacer` — `src/ui/assistant/pacer.zig:110-…`: 스트림 델타는 `enqueue`로 pending 버퍼에 쌓이고, `tick`이 경과시간×cps만큼 방출. cps는 백로그 적응형: `clamp(backlog/1.5s, 400..5000)`, 턴 종료 후엔 200ms 드레인 타깃(`computeCps:312-318`, 상수 `:10-13`). ANSI 시퀀스는 원자 방출(불완전 꼬리는 다음 틱, `emitN:339-…`). `SgrState`가 열린 속성을 추적해 프레임 사이에 다른 페인트가 `\x1b[0m`을 쏴도 다음 방출에서 재개방(`:40-108`, 주석 `:116-120`). 방출 대상은 transcript의 tail assistant 엔트리(`streamAssistantChunk` — `app_worker_runtime.zig:1874-1876`). 틱 위치: `loopCollectFacts` `main.zig:2582`, approval/question 활성 중엔 `pause`.
- **Thinking 표시**: 라벨 `"• Thinking" + 경과 + 토큰` — `src/core/output/activity_status.zig:26-33`; 대기(승인/질문) 중엔 시계 동결(`thinkingClockNow` 주석 `:37-40`); blink 500ms(`:35`). 페인트는 `shimmer_runtime.paintActivityIntoSurface`(`src/ui/transcript/shimmer_runtime.zig:151`, 입력 `ActivityPaintInput{label, shimmer_pos, thinking_blink}` `:16-24`) — shimmer_pos는 §3 애니메이션 phase에서 옴(`app_render_runtime.zig:1629-1636, 3453-3456`).
- **툴 활동 행**: `ToolLifecycleEvent` → `applyToolLifecycle`(`shell_runtime.zig:364-382` 파사드) → pinned status 엔트리 갱신 + `ActivityProjection { turn_thinking | tool_slot }`(`src/core/output/activity_runtime.zig:20-47`) → `activity_placement.resolve`가 activity band/overlay 위치 결정(`app_render_runtime.zig:3410-3417`), activity_painter로 surface에 칠함(`frame_builder.zig:254-258`).

## 6. Screens / overlays

- **배타 소유 alt screen 모달** (owner enum §1.1): 진입/이탈은 전부 `app_lifecycle.zig`의 쌍 함수 — approval `:766-770`, catalog `:799-805`, subagent manager `:830-844`, terminal session `:846-871`(진입 시 takeover reset + `2J H`), full transcript `:979-985`. 소유권 **핸드오프**(alt screen을 나가지 않고 owner만 교체): catalog→subagent `:807-811`, approval→subagent `:813-828`, full-transcript→approval `:948-969`, terminal-session→subagent `:875-892`.
- **approval_screen**: 파일 diff 승인. `needsScreen`이 diff 크기로 alt screen 필요 여부 판정(`src/ui/approval_screen.zig:208`), 아니면 footer 내 인라인 승인 UI(`footer/approval_ui.zig`). alt screen에서 인라인 복귀는 별도 write가 아니라 **다음 프레임의 terminal transition으로 합성**: `approvalInlineRestoreTransition` `app_lifecycle.zig:774-781` → `alternateScreenFrameRestoreSequence`(`1049l` + `[?25l`, `terminal.zig:22-27`)를 프레임 prefix로 실어 normal 화면 복귀+재페인트가 한 커밋에서 원자적으로 일어남.
- **카탈로그 화면들**(help/models/resume/settings/skills): 공통 형태 `Composer + PaintInput + paint()`(예: `src/ui/models_screen.zig:16-42`), 공용 레이아웃 `catalog_screen_layout.screenLayout`(`src/ui/catalog_screen_layout.zig:17-35`; composer 창 + 메뉴 예산 + divider/hint 행). catalog_menu owner의 alt screen 전체 화면으로 페인트되는 구조 `[추정: 다섯 모듈 모두 catalog_screen_layout를 쓰고 enterCatalogMenuScreen 경로가 유일한 catalog alt-screen 진입점이라는 정황; 개별 화면→owner 매핑 코드는 app_commands 쪽이라 직접 확인 안 함]`. 같은 메뉴들의 **인라인(슬래시 메뉴) 변형**은 `src/ui/footer/*_menu_presentation.zig`로 별도 존재.
- **종료 안전망**: `shutdownInteractiveShell`이 어떤 owner든 `leaveAlternateScreens`로 정리(`app_lifecycle.zig:622-631`).

## 7. Theme

- **시작 시 감지** `detectTheme` — `src/ui/terminal/theme_detection.zig:22-37`: `FX_THEME` env 오버라이드(`:15-20`) → OSC 11 질의(200ms 데드라인 바이트 루프, `:39-62`) → `COLORFGBG` 파싱(`theme_protocol.zig:68-76`) → 기본 dark. OSC 11 응답 파서/휘도 판정(>32768 → light): `theme_protocol.zig:11-40`. truecolor 게이트: COLORTERM 우선, Apple_Terminal 강등 — `theme_protocol.zig:44-53`. 적용은 전역 스타일 var 세트 교체 `initTheme` — `src/ui/render.zig:65-118`.
- **라이브 감지** `theme_monitor.Monitor` — `src/ui/terminal/theme_monitor.zig`: mode 2031 알림 활성(`shell_runtime.zig:230-233`). stdin 바이트 스트림의 **인터셉터**로 동작(`feed` `:80-…`): `\x1b[?997;1n/;2n`(dark/light) 인식 → dirty 마킹 → DA1 fence(`\x1b[c`)–OSC 11–fence 순서로 재질의(`takeQueryRequest` `:171-190`), 75ms/200ms 데드라인(`:7-8`), 매칭 실패 바이트는 입력 파서로 forward(`ForwardBytes`). 폴링/질의 발행은 `collectThemeFacts`(`main.zig:2456-2501`).
- **라이브 re-tint** `applyThemeUpdate` — `app_render_runtime.zig:343-357`: 저장된 엔트리의 SGR 재작성(`retintEntriesForTheme` → `store.zig`), pacer pending 버퍼의 inline-code 색 패치(`rethemeInlineCode` `pacer.zig:159-176`, 38;5;245↔247 치환), `initTheme`, terminal reset + transcript 재렌더 요청.

## 8. Minimum viable slice — Rust 포트 우선순위 제안

xfx 현재 상태(라인 append 셸)에서 fx 느낌으로 가는 결정적 도약은 **"normal 버퍼 하단 밴드 소유 + 2026 감싼 재페인트"** 하나다. 셀 diff·fixed-point·self-check는 그 위의 최적화/강건화 계층이라 뒤로 미룰 수 있다.

**P0 (이거 없으면 fx가 아님):**
1. Raw mode + interactive 모드 세트 + 정확한 종료/시그널 복원 (§1.2, §1.4). 복원 문자열은 upstream 상수를 그대로 이식 (`app_lifecycle.zig:36-44`).
2. 런치 커서 프로브(CSI 6n) + 기존 셸 출력 스크롤백 푸시 + row1 기점 뷰포트 (§1.3-1).
3. 하단 밴드 프레임: Layout(footer 4행 규칙 `terminal.zig:47-59`) + footer_layout + 8ms poll 루프 + render-request Reason 집합. 첫 버전 커밋은 "소유 밴드 전체를 `[?2026h…l` 안에서 다시 그림"으로 충분 — 2026이 플리커를 없애는 본체다 (`composeWireFrame` 구조만 차용).
4. Transcript 저장 + 워드랩 + visual-row 측정 + **overflow 시 document-append로 스크롤백 편입**(CUP bottom + `\n` 스크롤, CRLF 정규화) (§4, §1.3-2).
5. 스트리밍 pacer(적응 cps + SGR 상태 재개방) + Thinking/툴 activity 행 + 50ms 애니메이션 틱 (§5).
6. 시작 시 테마 감지(OSC 11 + COLORFGBG) + light/dark 팔레트 (§7 전반부).

**P1 (초기 릴리즈 품질):**
7. Shadow grid + `diffBand` 셀 diff + no-op skip (§2) — P0의 full-band repaint를 최소 바이트로 교체.
8. 리사이즈: SIGWINCH atomic flag + 디바운스 + full repaint (커서 프로브 fingerprint 페어링은 후순위).
9. approval alt screen + 프레임 합성 인라인 복귀 transition (§6).
10. OSC 2 title, bracketed paste, kitty keyboard(tmux 분기 포함).

**Defer (fx도 방어층으로 갖고 있는 것):**
- 커밋 self-check(shadow feed 대조·partial write 복구), frame retention, fixed-point 레이아웃 수렴(초기엔 "footer 먼저 측정→transcript" 1-pass로 근사), 라이브 테마 모니터(2031/997), full-transcript/subagent/terminal-session alt screen, tmux clear-history, Apple Terminal RIS, record tape, ui_observer, wasm.

**Rust crate 제안 (Zig가 손으로 하는 것에 근거):**
- **rustix** (termios + poll(2)) 또는 libc 직접 — fx의 터미널 계층 전부가 `tcgetattr/tcsetattr + poll + read/write` 뿐이다(`shell_runtime.zig:103-146, 250-278`). crossterm을 쓰더라도 **EventStream/이벤트 파서는 쓰지 말 것**: fx는 stdin 바이트 스트림을 자기 파서(escape_parser + cursor_probe + theme_monitor 인터셉터 체인, `main.zig:2681-2731`)로 소유해야 프로브 응답/테마 알림/kitty를 구분·유예할 수 있다. crossterm의 가치는 raw-mode RAII와 Windows 이식성 정도.
- **shadow grid는 자작(또는 fx engine.zig 포트)** — vt100/avt/wezterm-term 같은 범용 에뮬레이터보다, "우리가 쓴 바이트를 feed하면 정확히 우리가 의도한 grid가 되는" 경계형 엔진이 계약상 안전하다. fx가 자기 엔진을 쓴 이유가 명시돼 있다(`engine.zig:1-3`). 필요 서브셋: CUP/EL/ED/SGR(256+truecolor)/DECAWM/2026/OSC 8/wide+combining.
- **unicode-width + unicode-segmentation** — `display_width` 대응 (측정이 렌더와 1:1이어야 함, §4 측정).
- **비동기 런타임 불필요** — 단일 스레드 poll 루프. 에이전트 이벤트는 채널로 받아 collect_facts에서 `try_iter` drain (fx의 WorkerAppRuntime.tick과 동형).
- 하이라이터는 syntect 대신 **fx 렉서 포트**(4 토큰 클래스, `code_highlight.zig`) — 스트리밍 중 재렌더 비용과 출력 안정성 면에서 upstream과 동일 동작이 목표.

### 다음에 깨질 곳 (선제 경고)
- P0에서 셀 diff 없이 full-band repaint를 하면 tmux/원격(ssh) 저대역에서 프레임당 바이트가 커진다 — 2026 덕에 플리커는 없지만 대역이 문제되면 P1-7을 앞당겨야 한다.
- pacer 방출(transcript append)과 프레임 페인트가 같은 스크린 영역을 다루므로, SGR 재개방 규약(§5)을 빼먹으면 스트리밍 중 색이 샌다 — upstream이 주석으로 명시한 함정(`pacer.zig:116-120`).
- kitty push를 tmux에서 내보내면 키 입력이 깨진다 — 분기 필수(`terminal.zig:29-34`).
