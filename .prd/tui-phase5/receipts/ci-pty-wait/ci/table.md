| run / attempt | job | head | failing step | test | what failed | tests/tui.rs run (plain / fault injector) |
|---|---|---|---|---|---|---|
| 42942450215 / 1 | 130267239024 | c5cf594b | Tests, with the fault injector | `slash_skills_lists_installs_from_a_local_directory_and_never_from_the_network` | 20 s wait for `Skills 1`: `$foo` listed all 3 skills | 73.0 s / 83.5 s |
| 42939692388 / 1 | 130258279160 | 1012f418 | Tests | `a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing` | 20 s wait for `┃ xz`: the paste body was typed as keys | 84.5 s / not run |
| 42939692388 / 1 | 130258279160 | 1012f418 | Tests | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: `working` behind a frame showing `Generating`; no `Thinking` frame | 84.5 s / not run |
| 42939692388 / 2 | 130270115640 | 1012f418 | Tests, with the fault injector | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: reports were [idle, done], no `working` | 74.2 s / 77.9 s |
| 42939692388 / 3 | 130276081022 | 1012f418 | Tests, with the fault injector | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: `working` behind a frame showing `Generating`; no `Thinking` frame | 73.1 s / 78.2 s |
| 42880581900 / 1 | 130055471921 | dcfa5146 | The local-path check refuses what it exists for | - | not a PTY test (local-paths `shapes`, PR #95's class) | 75.1 s / 78.2 s |
| 42863297443 / 2 | 130015620008 | 4ad6405e | The local-path check refuses what it exists for | - | not a PTY test (local-paths `shapes`, PR #95's class) | 74.9 s / 78.4 s |
| 42692705003 / 1 | 129416346998 | df46777e | Tests | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: reports were [idle, done], no `working` | 72.7 s / not run |
| 42507366430 / 1 | 128812788998 | 58c92ba2 | Tests, with the fault injector | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: reports were [idle, done], no `working` | 57.3 s / 60.4 s |
| 42485857189 / 1 | 128747443572 | fdc5ac0b | Tests, with the fault injector | `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes` | 20 s wait: `Compaction cancelled.` still on screen behind MARKER-THREE | 58.0 s / 70.2 s |

Passing jobs' tests/tui.rs run time, seconds, by suite size (the suite grew 175 -> 189 cases and slowed on 2026-11-09):
- 0-187 plain cases: plain n=28, min 56.2, median 59.3, max 61.9; fault injector n=28, min 58.2, median 62.0, max 65.3
- 188-... plain cases: plain n=13, min 72.5, median 74.0, max 76.6; fault injector n=13, min 76.4, median 77.5, max 79.3
- failing jobs (PTY test failed): plain n=7, min 57.3, median 73.0, max 84.5; fault injector n=5, min 60.4, median 77.9, max 83.5
- jobs read: 50 completed aarch64-apple-darwin gate jobs, 41 passing

After the 50-job sweep: the 12 most recent CI runs again (`gh api .../actions/workflows/ci.yml/runs`,
2026-11-11 09:40Z), every macOS gate job of the runs and attempts the sweep had not read. Of those that
completed: aarch64-apple-darwin 7 passed, 2 failed, 2 cancelled; x86_64-apple-darwin checks 8 passed,
1 failed, 3 cancelled (release: 10 passed, 1 failed on `The @ search still fits inside a frame` in
run 42971215970, a step of the other rows' too and not a PTY test). The PTY failures:

| run / attempt | job | head | row, failing step | test | what failed | tests/tui.rs run |
|---|---|---|---|---|---|---|
| 42958837984 / 1 | 130320282880 | 88226763 | x86_64-apple-darwin checks, Tests, with the fault injector | `a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing` | 20 s wait for `┃ xz`: the paste body was typed as keys | 61.7 s (202 passed, 1 failed) |
| 42971215970 / 1 | 130359885817 | 12ea7867 | aarch64-apple-darwin, Tests | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: reports were [idle, done], no `working` | 72.3 s (187 passed, 1 failed) |
| 42986458620 / 1 | 130408681868 | 44c9d87f (release/v0.2.0, PR #96) | aarch64-apple-darwin, Tests | `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | assert: reports were [idle, done], no `working` | 72.8 s (188 passed, 1 failed) |
