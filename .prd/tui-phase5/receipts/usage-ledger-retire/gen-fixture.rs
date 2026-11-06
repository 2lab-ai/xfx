//! Throwaway: writes the US-U5 old-ledger fixture with the base build (120f7b20).
//! Run once: `LEDGER_FIXTURE_OUT=<dir> cargo test --test zz_gen_ledger_fixture -- --ignored`.
#![cfg(unix)]

mod support;

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use support::fake_gateway::{sse_body, sse_body_without_done, text_delta, FakeGateway, Reply};
use support::pty::{Pty, Session};
use support::sandbox::Sandbox;

const READY: &str = "\u{1b}[?2004h";

fn deferred(gen: &str, text: &str, at: &str) -> Reply {
    Reply::Sse(sse_body(&[
        json!({ "type": "response-metadata", "modelId": "fake/model-a", "timestamp": at }),
        text_delta("answer_0", text),
        json!({
            "type": "finish",
            "finishReason": { "unified": "stop", "raw": "stop" },
            "usage": {
                "inputTokens": { "total": 1200, "cacheRead": 200 },
                "outputTokens": { "total": 40, "reasoning": 0 }
            },
            "providerMetadata": { "gateway": { "generationId": gen } }
        }),
    ]))
}

fn wait(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn log_of(sandbox: &Sandbox) -> String {
    sandbox
        .session_ids()
        .first()
        .and_then(|id| fs::read_to_string(sandbox.sessions_dir().join(id).join("events.jsonl")).ok())
        .unwrap_or_default()
}

fn concluded(sandbox: &Sandbox) -> usize {
    log_of(sandbox).matches("\"kind\":\"turn_concluded\"").count()
}

fn gets(gateway: &FakeGateway) -> usize {
    gateway
        .requests()
        .iter()
        .filter(|r| r.method == "GET" && r.path.starts_with("/v1/generation?"))
        .count()
}

#[test]
#[ignore]
fn write_the_old_ledger_fixture() {
    let out = std::env::var("LEDGER_FIXTURE_OUT").expect("LEDGER_FIXTURE_OUT");
    let billed = Reply::Sse(sse_body(&[
        json!({ "type": "response-metadata", "modelId": "fake/model-a",
                "timestamp": "2026-11-06T12:00:01.000Z" }),
        text_delta("answer_0", "LEDGER-ONE billed answer"),
        json!({
            "type": "finish",
            "finishReason": { "unified": "stop", "raw": "stop" },
            "usage": {
                "inputTokens": { "total": 12000, "cacheRead": 4000 },
                "outputTokens": { "total": 800, "reasoning": 0 }
            },
            "providerMetadata": { "gateway": {
                "routing": { "canonicalSlug": "fake/model-a" },
                "cost": "0.0125",
                "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ0"
            } }
        }),
    ]));
    let record = json!({ "data": {
        "id": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1",
        "model": "fake/model-a",
        "created_at": "2026-11-06T12:00:02.000Z",
        "total_cost": 0.0042,
        "native_tokens_prompt": 1000,
        "native_tokens_completion": 40,
        "native_tokens_cached": 200,
        "native_tokens_cache_creation": 0,
        "native_tokens_reasoning": 0,
        "billable_web_search_calls": 0
    } })
    .to_string();
    let cut = Reply::SseThenAbort(vec![sse_body_without_done(&[
        json!({ "type": "response-metadata", "modelId": "fake/model-a",
                "timestamp": "2026-11-06T12:00:04.000Z" }),
        text_delta("answer_0", "LEDGER-FOUR cut short"),
    ])]);
    let gateway = FakeGateway::start(vec![
        billed,
        deferred("gen_01JABCDEFGHJKMNPQRSTVWXYZ1", "LEDGER-TWO settled later", "2026-11-06T12:00:02.000Z"),
        Reply::Sse(record),
        deferred("gen_01JABCDEFGHJKMNPQRSTVWXYZ2", "LEDGER-THREE rejected later", "2026-11-06T12:00:03.000Z"),
        Reply::Status(400, "bad generation".to_string()),
        cut,
        deferred("gen_01JABCDEFGHJKMNPQRSTVWXYZ3", "LEDGER-FIVE left pending", "2026-11-06T12:00:05.000Z"),
        Reply::Status(401, "not yours".to_string()),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    session.wait_for(READY);

    session.type_bytes(b"first billed turn\r");
    wait("turn 1", || concluded(&sandbox) == 1);
    session.type_bytes(b"second deferred turn\r");
    wait("turn 2", || concluded(&sandbox) == 2);
    wait("settled", || log_of(&sandbox).contains("\"settled\":["));
    session.type_bytes(b"third deferred turn\r");
    wait("turn 3", || concluded(&sandbox) == 3);
    wait("rejected", || log_of(&sandbox).contains("\"rejected\":["));
    session.type_bytes(b"fourth cut turn\r");
    wait("turn 4", || concluded(&sandbox) == 4);
    assert!(log_of(&sandbox).contains("\"unaccounted\":true"));
    session.type_bytes(b"fifth deferred turn\r");
    wait("turn 5", || concluded(&sandbox) == 5);
    wait("the 401", || gets(&gateway) == 3);
    std::thread::sleep(Duration::from_millis(500));
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    let ids = sandbox.session_ids();
    assert_eq!(ids.len(), 1, "{ids:?}");
    let from = sandbox.sessions_dir().join(&ids[0]);
    let to = Path::new(&out).join(&ids[0]);
    fs::create_dir_all(&to).expect("out");
    let ws = sandbox.workspace.display().to_string();
    let origin = format!("http://127.0.0.1:{}", gateway.port());
    for entry in fs::read_dir(&from).expect("dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if entry.file_type().expect("type").is_dir() {
            println!("DIR {}", name.to_string_lossy());
            continue;
        }
        let text = fs::read_to_string(entry.path()).expect("text");
        println!("FILE {} {} bytes", name.to_string_lossy(), text.len());
        fs::write(to.join(&name), text.replace(&ws, "@WS_A@").replace(&origin, "@ORIGIN@"))
            .expect("write");
    }
    println!("GETS {:?}", gateway.requests().iter().map(|r| format!("{} {}", r.method, r.path)).collect::<Vec<_>>());
    println!("ID {}", ids[0]);
}
