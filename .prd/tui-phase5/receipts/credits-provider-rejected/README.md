# credits-provider-rejected: the Gateway key beside a refused turn

A provider selection xfx cannot read keeps `provider_rejected` and leaves
`provider` on the compiled default, the Gateway (`src/config.rs:1108-1115`
@ `5693170f`). The field's contract is that it poisons the turn
(`src/config.rs:893-901`), and `Bundle::select` honours it
(`src/provider/mod.rs:343`). The shells still run, and four questions they
ask beside the turn read only `config.provider`, so they sent
`AI_GATEWAY_API_KEY` to the Gateway anyway.

## Every path that sends the Gateway key (`5693170f`)

| Path | Reads | Checks `provider_rejected` before | Now |
|---|---|---|---|
| `credits::question` (`src/credits.rs:148`); line shell `src/interactive.rs:1474`, TUI board `src/tui/worker.rs:1162`, `:1743` | `provider`, `credential` | no | refuses first, with `rejected_provider_help` on one line |
| `catalog_for` (`src/provider/model.rs:986`); TUI launch read and `/model` in both shells | `provider`, `credential` | no | `None`: no catalog |
| `Runtime::ask_for_bills` (`src/tui/worker.rs:1839`); resume and after every piece of work | `provider`, `credential` | no | asks nothing (`gateway_credential`) |
| `Runtime::title_asker` (`src/tui/worker.rs:1884`); the first prompt's title | `provider`, `credential` | no | asks nothing (`gateway_credential`) |
| `Bundle::select` (`src/provider/mod.rs:343`) | `provider_rejected` first | yes | unchanged |
| `prepare_gateway` (`src/provider/setup.rs:299-395`) | `resolve_credential_for` | n/a | unchanged: no I/O, the credential only becomes a source label; it is the command that repairs the setting |
| `status` auth label (`src/output.rs:132`), `gateway_catalog_note` (`src/tui/shell.rs:2226`) | `credential` | n/a | unchanged: labels, no request |

The bearer sinks are `src/credits.rs:195`, `src/provider/model.rs:948`,
`src/gateway/generation.rs:87` and `src/gateway/mod.rs:830`; each is
reached only through a row above.

## RED on `5693170f` (the tests of this branch, the code of the base)

Each pty case runs the debug binary against `FakeGateway` on loopback with
the test key and the profile `{"provider":"not-a-provider"}`.

- `credits::tests::an_unreadable_provider_asks_nobody_and_says_so_in_the_turns_words`: `asked the Gateway with a provider nobody chose`
- `provider::model::tests::an_unreadable_provider_has_no_catalog_to_ask_with_the_key`: `asked with the key`
- `tui::worker::tests::the_desk_asks_nothing_when_no_provider_was_validly_chosen`: `assertion failed: state.title_asker().is_none()`
- `tui::worker::tests::a_pending_bill_is_not_asked_for_when_no_provider_was_validly_chosen`: `the bill was asked for`, 30 asks against 0
- `tests/interactive.rs` `slash_credits_with_an_unreadable_provider_asks_nobody_and_says_why`: `["GET /coding-agent/v1/credits bearer=yes"]`
- `tests/interactive.rs` `slash_model_with_an_unreadable_provider_reads_no_catalog_with_the_key`: `["GET /coding-agent/v1/models bearer=yes"]`
- `tests/tui.rs` `an_unreadable_provider_sends_the_key_nowhere_beside_the_turn_it_refuses`: `["GET /coding-agent/v1/models bearer=yes", "POST /v3/ai/language-model bearer=yes", "GET /coding-agent/v1/credits bearer=yes"]` (catalog, title, credits)
- `tests/tui.rs` `a_pending_bill_resumed_under_an_unreadable_provider_is_not_asked_for`: `["GET /v1/generation?id=gen_01JABCDEFGHJKMNPQRSTVWXYZ3 bearer=yes", …]` twice

GREEN: all eight pass; each pty case passed alone three times running.

## Mutants (a `git clone --no-local` copy, one guard removed at a time)

- `credits::question` guard: the credits unit, line-shell `/credits` and the TUI case fail (`GET /coding-agent/v1/credits bearer=yes`); the other five pass.
- `catalog_for` guard: the catalog unit, line-shell `/model` and the TUI case fail (`GET /coding-agent/v1/models bearer=yes`).
- `gateway_credential` guard: both worker units, the TUI case (`POST /v3/ai/language-model bearer=yes`) and the resume case (`GET /v1/generation?id=… bearer=yes`) fail.
