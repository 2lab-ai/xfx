
#[test]
fn a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing() {
    // Train 9, join check 5 (PASTE-AFTER-END x QUESTIONS-20). A batch of
    // twenty questions (QUESTIONS-20's cap: `question_batch(20)`), Tab nine
    // times to page 10, its freeform slot opened (`3`, after the two choices)
    // and `keep` typed; then one write, so one delivery: a paste of `new`
    // with an Enter behind its end marker. fx v0.0.16 keeps the paste in the
    // field and drops what followed the marker (`paste_framing.zig:102-107
    // @481104fd`; the question's freeform owner, `app_input_runtime.zig:
    // 9718-9740`), then warns (`input_paste_runtime.zig:190-196`): the field
    // reads `keepnew`, the warning is written, nothing is answered and the
    // panel stays on question 10. An Enter of its own then takes `keepnew`
    // for question 10 and moves on to 11, and the batch, answered, reaches
    // the next request with it in its place.
    use support::fake_gateway::{content_only, finish, sse_body, tool_call, Reply};
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("q20", "ask_user_question", question_batch(20)),
            finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["MARKER-TWENTY"])),
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
    session.type_bytes(b"3");
    session.type_bytes(b"keep");
    on_screen(&session, (40, 120), &["3) keep"], &[]);
    session.type_bytes(b"\x1b[200~new\x1b[201~\r");
    on_screen(
        &session,
        (40, 120),
        &[
            "! input: Input that arrived right after the paste was ignored.",
            "3) keepnew",
            "Pick 10?",
            "Question 10 of 20",
        ],
        &["Pick 11?", "Paste was not applied"],
    );
    assert_eq!(
        gateway.request_count(),
        1,
        "the Enter behind the paste's end marker answered the batch"
    );

    // An Enter of its own takes the field for question 10; the walk goes on to
    // the next unanswered question, and nineteen ordinals answer the rest.
    session.type_bytes(b"\r");
    on_screen(&session, (40, 120), &["Pick 11?", "Question 11 of 20"], &[]);
    session.type_bytes(b"1111111111111111111");
    session.wait_for("MARKER-TWENTY");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));

    let requests = gateway.requests();
    assert_eq!(requests.len(), 2, "one panel, one answer, one more request");
    let output = tool_output(&requests[1].json(), "q20");
    assert_eq!(output["type"], "text", "{output:#}");
    let document: serde_json::Value =
        serde_json::from_str(output["value"].as_str().expect("a text result")).unwrap();
    let expected: Vec<serde_json::Value> = (1..=20)
        .map(|index| {
            serde_json::json!({
                "question": format!("Pick {index}?"),
                "answer": if index == 10 { "keepnew".to_string() } else { format!("Keep {index}") },
            })
        })
        .collect();
    assert_eq!(document, serde_json::Value::Array(expected));
}
