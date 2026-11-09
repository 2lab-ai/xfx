
#[test]
fn a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt() {
    // Train 9, join check 2 (QUESTIONS-20 x STOP-IN-CATALOG-WAIT). A stop at
    // a 20-question panel on its tenth page, by either key, ends the turn as
    // fx's `cancelQuestionPrompt` does (`input_question_runtime.zig:59-83
    // @481104fd`; `fx-question2-40x120-q2-esc`, and QUESTIONS-20's
    // `fx-p5-input-question-40x120-q6-after` for six): one `▪ Cancelled` row,
    // the panel gone, nothing more asked of the turn. The turn is kept, so
    // the next one's request carries the stopped prompt, the call, `aborted
    // by user` and `<turn_aborted>` (the one-question case:
    // `the_request_after_a_cancelled_question_answers_it_aborted_as_upstreams_does`).
    // The next turn reaching the Gateway at all is the other unit's half:
    // STOP-IN-CATALOG-WAIT's turn machine now ends a turn stopped before it
    // began unsent (`TurnMachine::run_journaled`), so a stop that outlived
    // the turn it was typed at would end the next one there.
    use support::fake_gateway::{content_only, finish, sse_body, tool_call, Reply};
    for (name, key) in [("Ctrl-C", &[0x03u8][..]), ("Escape", &[0x1b][..])] {
        let gateway = FakeGateway::start(vec![
            Reply::Sse(sse_body(&[
                tool_call("q20", "ask_user_question", question_batch(20)),
                finish("tool-calls"),
            ])),
            Reply::Sse(content_only(&["MARKER-NEXT"])),
        ]);
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        pty.resize(40, 120);
        let mut session =
            Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
        session.wait_for(READY);
        session.type_bytes(b"ask twenty\r");
        on_screen(&session, (40, 120), &["Pick 1?", "Question 1 of 20"], &[]);
        session.type_bytes(b"\t\t\t\t\t\t\t\t\t");
        on_screen(&session, (40, 120), &["Pick 10?", "Question 10 of 20"], &[]);
        session.type_bytes(key);
        on_screen(
            &session,
            (40, 120),
            &["\u{25aa} Cancelled"],
            &["Question 10 of 20", "Pick 10?"],
        );
        let screen = Screen::painted(&session.text(), 40, 120).expect("a frame");
        let cancelled: Vec<String> = (1..=40)
            .map(|row| screen.row_text(row))
            .filter(|row| row.contains("Cancelled"))
            .collect();
        assert_eq!(cancelled, ["\u{25aa} Cancelled"], "{name}");
        session.type_bytes(b"next\r");
        session.wait_for("MARKER-NEXT");
        session.type_bytes(b"/quit\r");
        assert_eq!(session.wait_exit().code(), Some(0), "{name}");

        let requests = gateway.requests();
        assert_eq!(requests.len(), 2, "{name}: the stopped turn, then the next");
        let body = requests[1].json();
        let prompt: Vec<serde_json::Value> = body["prompt"]
            .as_array()
            .expect("a prompt")
            .iter()
            .filter(|message| message["role"] != "system")
            .cloned()
            .collect();
        let roles: Vec<&str> = prompt
            .iter()
            .map(|message| message["role"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            roles,
            ["user", "assistant", "tool", "user", "user"],
            "{name}: {body:#}"
        );
        assert_eq!(
            prompt[0]["content"][0]["text"], "ask twenty",
            "{name}: {body:#}"
        );
        assert_eq!(
            prompt[1]["content"][0]["type"], "tool-call",
            "{name}: {body:#}"
        );
        assert_eq!(
            prompt[1]["content"][0]["input"]["questions"]
                .as_array()
                .map(Vec::len),
            Some(20),
            "{name}: {body:#}"
        );
        assert_eq!(
            tool_output(&body, "q20"),
            serde_json::json!({ "type": "error-text", "value": "aborted by user" }),
            "{name}: {body:#}"
        );
        assert!(
            prompt[3]["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("<turn_aborted>")),
            "{name}: {body:#}"
        );
        assert_eq!(prompt[4]["content"][0]["text"], "next", "{name}: {body:#}");
    }
}
