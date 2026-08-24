# fx upstream 최근 델타 — pin 580a0c5 → HEAD ef1d0d0 (0.0.5)

작성 2026-08-24. 소스: 업스트림 클론 `scratchpad/fx-src` (shallow, HEAD ef1d0d0 = PR #381 머지),
`CHANGELOG.md` 0.0.5 섹션, `README.md`; xfx 측은 `/Users/zhugehyuk/2lab.ai/xfx/docs/parity.md`(정본 원장)와
`UPSTREAM.md`(pin 선언 + 의도적 편차 9종).

**범위 주의**: 클론이 depth-1 shallow라 pin 커밋이 0.0.4/0.0.5 경계 어디에 있는지 git으로 확정 불가.
지시대로 0.0.5 섹션 전체를 델타로 취급하고, 지시에 명시된 0.0.2~0.0.4 항목(`/permissions remember|revoke`,
auto-mode review 계보)도 포함했다. xfx 원장이 이미 반영한 항목은 표에서 "수렴"으로 표시. `file:line`은
별도 표기 없으면 업스트림 클론 기준.

## 1. 델타 테이블

| # | 업스트림 동작 (근거) | xfx 상태 (parity.md 근거) | 관련성 / 판단 |
|---|---|---|---|
| B1 | **호스트 커맨드 실행 — 샌드박스 전면 은퇴.** 승인된 captured/background/monitor 커맨드를 평범한 호스트 서브프로세스로 실행, sandbox 설정·status 필드·커맨드 제거 (CHANGELOG 0.0.5 Breaking; `config_runtime.zig:3011` "legacy sandbox keys are inert unknown data" 테스트) | **수렴 (xfx가 먼저 도착).** parity "OS command sandbox — deferred", UPSTREAM.md 편차 #2 "`sandbox` is always `none`" | **HIGH.** xfx가 "샌드박스 없음을 정직하게 보고"하던 지점에 업스트림이 합류. 남은 일은 코드가 아니라 원장: `status`의 sandbox 필드·편차 #2가 이제 편차가 아님 → 리베이스 시 parity/UPSTREAM 갱신, `status` JSON 계약 재검토 |
| B2 | **프로바이더 전환이 `/setup`으로 이동, `/provider` 슬래시 제거** (CHANGELOG Breaking; `auth_runtime.zig:493` "Switch provider" 픽커, `picker_presentation.zig:95`; README "open `/setup` and choose **Switch provider**") | absent. parity `provider` command — deferred, `setup` command — implemented but llmux-only, `/setup` slash — deferred | **HIGH.** 계획 중인 프로바이더 아키텍처의 목표 표면이 확정됨: `/provider`를 만들면 안 되고 `/setup` 픽커가 정본. `fx provider` top-level 커맨드는 잔존 |
| N1 | **Codex 구독 로그인** `fx login codex` — OAuth, 세션 `~/.fx/chatgpt-auth.json`, 인증 카탈로그 모델, `/fast` = priority tier (CHANGELOG New; README; `src/core/auth/chatgpt_session.zig` 존재) | absent. parity "Codex / ChatGPT subscription — deferred", `login` — deferred | **HIGH.** 이미 계획된 축(Codex OAuth). 델타가 추가로 확정한 계약: 로그인 직후 카탈로그-유효 모델 자동 활성화, 인증 URL 클릭 가능한 터미널 링크, macOS Keychain 저장(Security 항목 S2) |
| N2 | **Grok 구독 로그인** `fx login grok` — xAI OAuth, `~/.fx/grok-auth.json`, effort levels, Responses API (CHANGELOG New; README; `src/core/auth/grok_session.zig`) | absent. 동일 deferred 군 | **HIGH.** Grok OAuth도 계획 축. effort level이 카탈로그 1급 개념이 된 근거 제공자 |
| N3 | **워크스페이스 상태줄** — `/settings`, `/statusline workspace`, `statusLine.workspace`로 옵트인, 경로+Git 브랜치 표시 (CHANGELOG New; `config_runtime.zig:58` `statusline_workspace`, `:529` statusLine 파싱, `:2962-2967` merge 테스트; README) | absent. parity "notifications and status line — deferred" | **MED-HIGH.** TUI 포트의 상태줄 스펙에 직결. 기본 숨김+옵트인이라는 프라이버시 결정까지 계약의 일부 |
| N4 | **fx-네이티브 워크스페이스 스킬** — `.fx/skills`를 다른 워크스페이스·호환 루트보다 먼저 발견. 순위: managed `~/.fx/skills`=0 < workspace `.fx/skills`·shared=1 < 호환 루트(`.claude/.agents/.codex/.opencode/.claw`)=2 (`skill_runtime.zig:911-928` `skillGroupRank`; `skill.zig:193`) | absent. parity "skills (`skill`, `install_skill`) — deferred" | **MED.** xfx에 스킬이 들어갈 때 네이티브 루트는 `.xfx/skills`가 될 것 — 발견 순위 구조를 처음부터 이식 |
| N5 | **외부 스킬 심링크 권위** `FX_SKILL_SYMLINK_AUTHORITIES` — 콜론 구분 절대경로 목록 안으로 해석되는 심링크만 허용 (`skill_runtime.zig:448-454`; README "e.g. Nix store paths") | absent (스킬 자체 deferred) | **LOW.** Nix 등 니치. 스킬 구현 시 함께 오는 보안 세목이지 독립 우선순위 아님 |
| I1 | **프로바이더 셋업 개선** — 구독 로그인 후 카탈로그-유효 모델 활성화, 로그아웃된 프로바이더 `/setup`에서 재인증, Codex 인증을 클릭 가능한 링크로 (CHANGELOG Improvements) | absent (N1/N2와 동일 군) | **MED.** N1/N2 구현의 수용 기준 목록으로 흡수 |
| I2 | **`/model` 카탈로그 표시** — 프로바이더가 광고한 모델·컨텍스트 윈도·effort level을 `/model`과 상태줄에 표시 (`model_menu_presentation.zig:279-286` context 토큰 표시; `picker_presentation.zig:456,520` effort 스테이지, `:1008` "/model — choose what model and reasoning effort to use") | partial. parity `/model` — implemented지만 "does not browse a catalog"; `models` command — deferred | **HIGH.** xfx는 이미 `setup llmux`에서 `/models` 카탈로그를 읽는다(parity setup 행) — 대화형 표면만 없음. TUI 포트에서 체감 1순위급 |
| I3 | **세션 목록 UX** — 저장된 세션 이름, 읽기 쉬운 UTC 타임스탬프, 언어 이름, 단수형 turn 카운트 (CHANGELOG Improvements) | partial. parity `sessions` — implemented(JSON/텍스트, 결정적)이나 세션 **이름** 개념 자체가 없음(`/rename` deferred) [추정: xfx 렌더러의 타임스탬프 포맷은 미대조] | **MED.** `fx sessions`/`session resume` UX 정렬. 이름은 `/rename` 도입과 한 묶음 |
| I4 | **세션 캐시 읽기** — 다른 세션이 캐시 발행을 미루는 동안에도 목록·latest-resume 응답성 유지 (CHANGELOG) | 수렴(다른 설계). parity "session manifest and index": xfx는 staged manifest+RAII, 락 경합 시 거부-아님-대기 설계를 이미 가짐 | **LOW.** 아키텍처가 달라 1:1 이식 대상 아님; 동시성 수용 기준으로만 참고 |
| I5 | **터미널 탭 제목** — 세션명(폴백: 워크스페이스명)+활성 모델, rename/resume/모델 변경 추적, 종료 시 클리어, 비대화형은 미발신 (`app_session_runtime.zig:2975-3026` 제목 조립: primary 64B+` · `+model 48B; README) | absent. parity 원장에 행 없음 — xfx 라인 셸은 OSC 자체를 안 씀("The one control sequence xfx emits is the erase pair") | **MED-HIGH.** TUI 포트에서 저비용·고체감. herdr 같은 멀티플렉서 사용자(=우리)에게 특히 유효 |
| I6 | **터미널 액티비티 행** — 커맨드/셸을 완료까지 액티비티 행에 붙잡고, graceful/force close 구분, `cd . &&` 접두 숨김 (CHANGELOG) | absent. parity "durable terminal sessions — deferred" | **MED.** durable terminal 구현 시의 UX 계약 |
| I7 | **터미널 액션 인자** — 선택된 액션에 관련된 필드만 광고, 미저장 `fx ask` 세션은 `terminal.exec`만 (CHANGELOG) | 수렴. parity `terminal` — implemented "exec action only" (xfx는 항상 exec-only) | **LOW.** durable 액션 추가 때 스키마-축소 패턴만 기억 |
| I8 | **auto 모드 읽기** — 루틴 read-only 커맨드와 hardened Git 조회를 자동 리뷰 없이 직접 실행 (CHANGELOG; `command_effect.zig:1297` "planner pins read-only git inspection to hardened argv and environment") | 수렴(더 좁게). parity "automatic command grammar — partial": xfx auto는 리뷰어 자체가 없고 보고-전용 문법만 직접 실행, git은 `-c core.fsmonitor=false` 등 동일한 경화 적용(parity `terminal` 행) | **HIGH.** §3 참조 — 리뷰어 도입 시 "직접 실행 층"과 "리뷰 층"의 2층 구조가 계약 |
| I9 | **자동 거부 회복** — 파괴적 액션은 에이전트에 되돌려 재계획, 반복 무진전 거부는 승인 프롬프트 대신 일반 어시스턴트 출력으로 턴 종료 (CHANGELOG; `tool_admission.zig:4938` caution→recoverable hold, `:4984,:5023` invalid review→에이전트 복귀, `runtime/tests/tool_flow.zig:486` "Blocked by automatic safety policy") | absent. xfx는 자동 리뷰가 없으므로 이 상태기계 전체가 부재 | **HIGH.** §3 참조 — 퍼미션 엔진의 턴 종료 계약 |
| I10 | **one-off 서브에이전트** — 활성 동안 가시 유지, 최종 결과 1회 전달, 완료 후 은퇴; persistent는 재사용 가능 (`subagent/domain.zig:26` `one_off` 모드, `:763` one-off는 prompt 필수, `:1877-1889` 라이프사이클 전이) | absent. parity "`subagent` — deferred" | **MED.** 서브에이전트 도입 시 one-off/persistent 이원 모델과 라이프사이클 상태기계를 그대로 이식 |
| I11 | **시작 시 환경설정** — 모델 capability 로딩 중에도 저장된 reasoning effort·Fast 모드 즉시 표시 (CHANGELOG) | absent | **LOW.** TUI 폴리시; capability 비동기 로딩이 생긴 후의 문제 |
| I12 | **dev 빌드 정체성** — dev 채널 환영 헤더에 커밋+`[dev]` 마커 (`render.zig:173-184`, `:923` `v0.0.5-abcdef1 [dev]`) | partial-수렴. xfx는 `build_channel`+12자 리비전을 `/version`·status에 이미 보고(UPSTREAM.md 편차 #3, parity `/version`) — 환영 헤더 개념이 없을 뿐 | **LOW.** TUI 헤더 만들 때 1줄 |
| I13 | **MCP 리로드 피드백 / 도움말 레이아웃 / 바이너리 크기** (CHANGELOG) | absent (MCP deferred) / n.a. | **LOW.** |
| I14 | **안정판 업그레이드 + Ctrl+G** — 수동·자동·Ctrl+G 업그레이드에서 forward-only 버전 순서 복원; Ctrl+G(0x07)는 준비된 업그레이드 적용 단축키로 모달 위에서도 동작 (`app_input_runtime.zig:77` `ctrl_g_upgrade_byte`, `:1148-1165` `routeUpgradeShortcut`/`applyReadyUpgradeShortcut`) | absent — 의도적. parity `upgrade` — deferred, UPSTREAM.md 편차 #3 "xfx has no updater" | **LOW.** xfx가 업데이터를 갖기 전까지 무의미; 갖게 되면 forward-only 순서 보장이 계약 |
| F1 | **비정규 파일 읽기 거부** — FIFO 등 non-regular `read_file` 대상을 블록되기 전에 거부 (CHANGELOG Bug Fixes) | **absent — 실제 갭 후보.** xfx `src/tools/read.rs:841-847`은 디렉터리만 거부(`metadata.is_dir()`), FIFO 가드 grep 무히트 [추정: xfx read 툴에 FIFO를 열면 블록 가능] | **HIGH(저비용).** 정확성 수정이고 Rust에서 몇 줄. 포트가 업스트림보다 덜 안전한 유일한 항목일 수 있음 |
| F2 | **malformed 툴 루프** — 3연속 malformed-only 툴 배치 후 턴 종료, 유효 배치 후 리셋 (CHANGELOG) | absent [추정: `xfx/src/agent` grep "malformed" 무히트] | **MED.** 프로바이더 불문 에이전트 루프 견고성; 저비용 |
| F3 | **자격증명 폴백** — `fx login` 자격증명 로드/갱신 실패 시 저장된 API 키로 계속, 로그인 실패는 진단으로 보존 (CHANGELOG) | absent (저장 자격증명 자체가 deferred) | **MED.** OAuth 구현 시의 폴백 사다리 계약: OAuth → stored key → env |
| F4 | **oversized 이미지 / 손상 메모리 저장소 / Vision 재시도 / thinking 인디케이터 / 터미널 헬퍼 호환 / WASM / idle 테마 폴링** (CHANGELOG) | absent — 전부 deferred 표면(vision, memory, TUI, WASM)에 종속 | **LOW.** |
| S1 | **커맨드 승인 패턴** — 와일드카드 allow를 정적 셸 워드로 제한, 파괴적 셸 커맨드·파일 삭제는 자동 리뷰 범위 밖 유지 (CHANGELOG Security) | partial-수렴. xfx는 glob 그랜트 자체가 absent(parity "permission rules and grants — partial": exact tool+target만), 파괴적 커맨드는 auto 문법이 원천 배제 | **HIGH.** §3 참조 — 규칙 문법을 넓힐 때의 안전 하한선 |
| S2 | **macOS 로그인 저장** — 네이티브 `fx login` 세션을 Keychain에, 마이그레이션·갱신·재시작·로그아웃 검증 (CHANGELOG Security) | absent | **MED.** Codex/Grok OAuth 구현의 저장소 결정에 직결: 평문 `~/.xfx/*-auth.json` vs Keychain |
| S3 | **프로바이더 응답 한계** — oversized Codex/Grok 카탈로그·스트림·툴 데이터·리플레이 상태 거부, 이후 입력은 유지 (CHANGELOG Security) | 수렴(하우스 스타일). xfx llmux/Gateway 디코더는 이미 "bounded per frame, per completion, and by tracked-block count" (parity `llmux` provider 행) | **MED.** 새 프로바이더(Codex/Grok) 클라이언트에도 같은 bounded-decode 규율 적용하라는 확인 |
| S4 | **MCP 설정 원자적 쓰기 / MCP 세션 은퇴 / ACP 퍼미션 검증** (CHANGELOG Security) | absent (MCP·ACP deferred) | **LOW.** |
| P1 | **`/permissions remember\|revoke`** (0.0.2 도입, 이후 계속 확장) — 세션 내 `remember <allow\|deny> <tool> <args-json>`으로 실행 없이 exact 규칙 저장, 안정 ID로 목록, 원래 워크스페이스·파일 상태가 변해도 `revoke <rule-id>` 가능 (`session_commands.zig:24` usage 문자열, `picker_presentation.zig:1011` `/permissions [ask\|auto\|remember\|revoke\|yolo\|reset]`; README) | partial. parity "permission rules and grants — partial": xfx는 in-memory exact 규칙+"always" 세션 그랜트(durable, 세션 id 스코프)는 있으나 "Configured rules are still not read from or written to settings"; `/permissions` slash·command 둘 다 deferred | **HIGH.** §3 참조 |
| P2 | **`/feedback`** — fx.sh/feedback 열기, 진단 미생성 (`commands.zig:442`; `app_commands.zig:519`) | absent (slash deferred 군) | **LOW.** 업스트림 제품의 피드백 폼 — 포트가 가리킬 곳이 아님. 이식 비대상 판단 |
| P3 | **`/trace`** — 로그·세션 컨텍스트·런타임 상태·퍼미션·최근 활동을 담은 사설 Markdown 진단, macOS는 클립보드 복사 (`commands.zig:443`; README "Review and redact the trace before sharing") | absent | **MED.** xfx의 "정직한 자기보고" 문화(`doctor`)와 결이 같음. `doctor`의 대화형 확장으로 저비용 구현 가능 |

## 2. 포팅 우선순위 제안

전제: TUI 포트, 프로바이더 아키텍처(Codex/Grok OAuth + llmux), PRD 작업이 이미 계획됨. 아래는 "현행 fx의
대화형 경험에 수렴"하는 absent 동작 중 가치순 상위 ~15. 앞 번호가 PRD에서 먼저 소비될 항목.

1. **`/setup` 프로바이더 픽커를 전환 표면으로 확정** (B2, I1) — 아키텍처 결정이 무료로 확정됐다: `/provider`는 죽은 표면이니 만들지 말 것. llmux를 Gateway·Codex·Grok과 동렬의 픽커 항목으로 설계하면 xfx 고유 백엔드가 업스트림 UX에 자연 편입된다.
2. **Codex OAuth 로그인** (N1) — 계획 축의 수용 기준이 델타로 구체화됨: 로그인→카탈로그-유효 모델 자동 활성화, 클릭 가능한 인증 링크, 폴백 사다리(F3).
3. **Grok OAuth 로그인** (N2) — 같은 프로바이더 패밀리 작업에서 한계비용이 낮고, effort-level 개념의 원천 공급자.
4. **`/model` 카탈로그 브라우즈 + 컨텍스트 윈도·effort 표시** (I2) — 카탈로그 소스(llmux `/models`)는 xfx에 이미 있다. TUI에서 사용자가 매일 만지는 표면이라 체감 대비 비용이 가장 좋다.
5. **auto 모드 자동 안전 리뷰 파이프라인** (I8, §3) — 미해결 액션당 1회의 좁은 리뷰, clear=그 액션만 인가, caution/unavailable=프롬프트 없이 hold+조언. xfx의 one-use authority 설계와 정확히 맞물리는 확장점.
6. **자동 거부 회복 상태기계** (I9, §3) — 파괴적 액션은 재계획으로 반환, 반복 무진전 거부는 일반 출력으로 턴 종료. 5와 한 몸: 리뷰어를 넣는 순간 이 회복 계약 없이는 스톨이 생긴다.
7. **`/permissions remember/revoke` — 규칙의 프로세스 간 영속화** (P1, §3) — xfx의 in-memory 규칙+durable "always" 그랜트에서 반 발짝. 안정 rule-ID와 상태-무관 revoke가 계약의 핵심.
8. **와일드카드 allow의 정적-셸-워드 제한 + 파괴적 커맨드의 자동 리뷰 배제** (S1, §3) — 7에서 규칙 문법을 넓히는 순간 필요한 안전 하한선. 규칙 엔진과 동시 설계해야 후장착 비용이 없다.
9. **터미널 탭 제목 (OSC)** (I5) — 세션/워크스페이스명+모델, 종료 시 클리어. TUI 포트에서 수십 줄짜리 저비용·고체감. 멀티플렉서(herdr) 환경에서 가치 증폭.
10. **워크스페이스 상태줄 `statusLine.workspace`** (N3) — TUI 상태줄 스펙의 일부로. 기본 숨김+3경로 옵트인(`/settings`·슬래시·설정키)이 계약.
11. **non-regular `read_file` 거부** (F1) — 유일하게 "업스트림이 고쳤는데 xfx에 남아 있을 수 있는" 정확성 결함 [추정]. Rust로 몇 줄, 우선순위 대비 비용 최소라 즉시 처리 후보.
12. **세션 목록 UX + `/rename`** (I3) — 세션 이름·읽기 쉬운 UTC 시각·turn 카운트. xfx `sessions`가 이미 견고하므로 렌더링+이름 이벤트 추가가 전부.
13. **malformed 툴-루프 컷오프** (F2) — 3연속 malformed-only 배치에서 턴 종료. 프로바이더 불문 견고성, 에이전트 루프에 국소적.
14. **`/trace` 진단** (P3) — doctor의 대화형 확장. 포트 사용자 지원(버그 리포트)에 직접 기여하고 xfx의 자기보고 문화와 일치.
15. **one-off 서브에이전트 라이프사이클** (I10) — 서브에이전트 도입 시점에 one-off/persistent 이원 모델과 visible→final-result→retire 전이를 처음부터 이식. 서브에이전트 자체가 후순위라 목록 말미.

차상위(이번 PRD 비대상 권고): `.fx/skills` 우선순위 구조(N4)와 `FX_SKILL_SYMLINK_AUTHORITIES`(N5)는 스킬
도입 시점에 세트로; Keychain 저장(S2)은 2·3의 구현 결정 사항으로 흡수; Ctrl+G/업그레이드(I14)는 xfx가
업데이터 부재를 의도로 선언한 동안 비대상; `/feedback`(P2)은 이식하지 않는 것이 맞다(업스트림 제품 폼).

## 3. 퍼미션 모델 변화 — 우리 퍼미션 엔진 계약에 주는 영향

업스트림의 현행 auto 계약 (README, 0.0.2→0.0.4→0.0.5 누적):

1. **2층 구조.** 아래층 = 자동 리뷰 없이 직접 실행되는 층: 루틴 read-only 커맨드, hardened Git 조회
   (`command_effect.zig:1297` — 경화된 argv+환경으로 고정), 준비된 워크스페이스 편집, 되돌릴 수 있는 루틴
   개발 커맨드와 신규 파일 생성(0.0.4). 위층 = **미해결 액션당 정확히 1회의 좁은 자동 안전 리뷰** — 입력은
   "현재 사용자 요청 + 정확한 대기 액션"이고, clear 결과는 **그 액션 하나만** 인가한다 (README "A clear
   result authorizes only that action"; `tool_admission.zig:4608` "mints matching one-call authority").
2. **리뷰 결과의 3상.** clear=실행 / caution·unavailable=**프롬프트를 열지 않고, 턴을 끝내지도 않고**,
   액션을 hold하고 조언을 에이전트에 반환 (`tool_admission.zig:4938`; `:943` "An unavailable or invalid
   automatic review never executes anything"). invalid 리뷰도 프롬프트 전에 에이전트로 복귀 (`:4984`).
3. **거부 회복 (0.0.5).** 파괴적 액션은 에이전트에 되돌려 재계획시키고, 반복 무진전 거부는 승인 프롬프트
   대신 일반 어시스턴트 출력으로 턴을 정상 종료한다 (CHANGELOG I9). 즉 "auto 모드는 사람을 호출하지
   않는다"가 불변식이고, 사람 승인은 `ask` 모드나 `--prompt-permissions`(0.0.4, TTY 한정, 자동 리뷰는 그
   프롬프트를 절대 열지 않음)의 몫.
4. **영속 규칙 (0.0.2+).** `/permissions remember <allow|deny> <tool> <args-json>` = 실행 없이 exact 규칙
   저장, 안정 rule-ID 목록, 원 워크스페이스·파일 상태가 변해도 revoke 가능 (`session_commands.zig:24`,
   README). 저장 규칙이 있으면 자동 리뷰를 건너뛴다 (`tool_admission.zig:5059` "configured command
   authority skips automatic review").
5. **안전 하한선 (0.0.5 Security).** 와일드카드 커맨드 allow는 정적 셸 워드로만; 파괴적 셸 커맨드와 파일
   삭제는 무엇이 있어도 자동 리뷰 범위 밖 (CHANGELOG S1).
6. **샌드박스 은퇴 (0.0.5 Breaking).** 승인 후에는 OS 격리 없이 호스트 서브프로세스로 실행 — 승인 결정
   층이 유일한 방어선이 됐다 (B1).

xfx 엔진에의 함의:

- **xfx의 현행 auto는 업스트림의 "아래층"만 있는 형태다** (parity "automatic command grammar — partial";
  UPSTREAM.md 편차 #9 "no automatic review, never widens itself"). 리뷰어를 도입한다는 것은 문법을 넓히는
  게 아니라 **위층을 추가**하는 것 — 아래층의 보고-전용 문법은 그대로 두고, 그 밖의 액션이 "거부" 대신
  "리뷰 1회"를 받게 하는 구조 변경이다.
- xfx의 **one-use authority 민팅**(parity "Decisions mint one-use authorities that are spent before they
  are revalidated")은 업스트림의 "clear authorizes only that action / one-call authority"와 동형이라,
  리뷰어의 출력 타입을 기존 authority 타입에 그대로 접합할 수 있다. 계약 변경이 아니라 authority 발급자의
  추가다.
- **턴 종료 계약이 바뀐다.** 현행 xfx auto의 거부는 툴 결과로 돌아가 에이전트가 알아서 하지만, 업스트림
  계약은 (a) hold+조언 반환, (b) 반복 무진전 시 도구-없는 일반 출력으로 턴 정상 종료라는 명시적
  상태기계다. bounded-turn 보장(xfx 편차 #1의 step limit)과의 상호작용 — 리뷰 hold가 스텝을 소모하는가 —
  를 PRD에서 정의해야 한다 [추정: 업스트림의 스텝 회계는 이번 조사로 미확정].
- **규칙 영속화는 반 발짝 거리다.** xfx는 이미 exact `tool`+`target` 규칙, cwd-키잉된 커맨드 그랜트,
  세션-id 스코프의 durable "always"를 가진다(parity "permission rules and grants — partial"). 업스트림과의
  차이는 (i) 설정 파일로의 영속·로드, (ii) 안정 rule-ID, (iii) 상태-무관 revoke, (iv) glob 그랜트.
  (iv)를 여는 순간 S1의 정적-셸-워드 제한이 전제조건이다.
- **샌드박스 은퇴는 xfx의 정당화를 뒤집는다.** 지금까지 "업스트림은 OS 샌드박스가 있는데 xfx는 없으므로
  auto를 좁게"였다면, 이제 업스트림도 승인-층이 유일 방어선이다. 즉 xfx가 auto를 업스트림 수준으로 넓히는
  것의 안전 논거가 대칭이 됐다 — 넓힐지는 여전히 리뷰 결정이지만, "업스트림엔 샌드박스가 있어서"라는
  비대칭 논거는 소멸했다. UPSTREAM.md 편차 #2·#9의 문구 갱신 필요.
