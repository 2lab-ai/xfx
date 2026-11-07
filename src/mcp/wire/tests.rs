//! fx's own tests of its decoder, ported (`vercel-labs/fx@481104fd
//! src/mcp/protocol/wire.zig:509-700`), with the depth fx's release build
//! takes.

use super::*;

fn response(input: &str) -> Response<'_> {
    match decode(input).expect("decodes") {
        Message::Response(r) => r,
        other => panic!("not a response: {other:?}"),
    }
}

#[test]
fn decodes_results_with_numeric_string_and_foreign_ids() {
    let numeric =
        response(r#"{"jsonrpc":"2.0","id":7,"result":{"tools":[],"resultType":"complete"}}"#);
    assert_eq!(numeric.id, Some(7));
    assert_eq!(
        numeric.body,
        Body::Result {
            raw: r#"{"tools":[],"resultType":"complete"}"#,
            kind: ResultKind::Complete
        }
    );
    assert_eq!(
        response(r#" {"result":{},"id":"12","jsonrpc":"2.0"} "#).id,
        Some(12)
    );
    for foreign in [
        r#""abc""#,
        r#""012""#,
        r#""1_0""#,
        "-3",
        "null",
        "1.5",
        "99999999999999999999",
    ] {
        let input = format!(r#"{{"jsonrpc":"2.0","id":{foreign},"result":{{}}}}"#);
        assert_eq!(response(&input).id, None, "{input}");
    }
    assert_eq!(response(r#"{"jsonrpc":"2.0","result":{}}"#).id, None);
}

#[test]
fn an_absent_result_type_is_complete_and_an_unknown_one_is_unrecognized() {
    for (input, kind) in [
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            ResultKind::Complete,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"input_required","requestState":"x"}}"#,
            ResultKind::InputRequired,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"task"}}"#,
            ResultKind::Unrecognized,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":7}}"#,
            ResultKind::Unrecognized,
        ),
        // Raw, as fx compares it: an escaped "complete" is not "complete".
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"compl\u0065te"}}"#,
            ResultKind::Unrecognized,
        ),
    ] {
        match response(input).body {
            Body::Result { kind: got, .. } => assert_eq!(got, kind, "{input}"),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn decodes_errors_including_the_specs_unsupported_version_example() {
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32022,\"message\":\"Unsupported protocol version\",\n \"data\":{\"supported\":[\"2026-07-28\",\"2025-11-25\"],\"requested\":\"1900-01-01\"}}}";
    let Body::Failure(failure) = response(input).body else {
        panic!("a failure")
    };
    assert_eq!(failure.code, -32022);
    assert_eq!(failure.message, "\"Unsupported protocol version\"");
    assert!(failure.data.expect("data").starts_with("{\"supported\":["));
    let no_id =
        response(r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}"#);
    assert_eq!(no_id.id, None);
}

#[test]
fn a_refused_line_shaped_like_a_response_still_names_its_request() {
    for line in [
        r#"{"id":3,"result":{}}"#,
        r#"{"jsonrpc":"1.0","id":3,"result":{}}"#,
        r#"{"jsonrpc":"2.0","id":"3"}"#,
        r#"{"jsonrpc":"2.0","id":3,"result":{},"error":{"code":1,"message":"x"}}"#,
    ] {
        assert!(decode(line).is_err(), "{line}");
        assert_eq!(response_id(line), Some(3), "{line}");
    }
    // A request, a null or foreign id, and a line that isn't an object name
    // none.
    for line in [
        r#"{"id":3,"method":"ping","jsonrpc":"1.0"}"#,
        r#"{"id":null}"#,
        r#"{"id":-1}"#,
        "[3]",
        "not json",
    ] {
        assert_eq!(response_id(line), None, "{line}");
    }
}

#[test]
fn a_key_written_with_escapes_never_matches_a_field() {
    let message = response(r#"{"jsonrpc":"2.0","x\u0000id":5,"id":1,"result":{}}"#);
    assert_eq!(message.id, Some(1));
    // Nor does one that only spells "id" with an escape.
    assert_eq!(
        response(r#"{"jsonrpc":"2.0","i\u0064":5,"result":{}}"#).id,
        None
    );
}

#[test]
fn decodes_string_tokens_with_or_without_escapes() {
    assert_eq!(
        decode_string(r#""https://x/y""#).as_deref(),
        Some("https://x/y")
    );
    assert_eq!(
        decode_string(r#""https:\/\/x\/y\u00e9""#).as_deref(),
        Some("https://x/y\u{e9}")
    );
    assert_eq!(decode_string("42"), None);
    assert_eq!(decode_string(r#""a\x""#), None);
    assert_eq!(plain_string(r#""2025-06-18""#), Some("2025-06-18"));
    assert_eq!(plain_string(r#""2025\u002d06-18""#), None);
    assert_eq!(plain_string("7"), None);
}

#[test]
fn finds_a_plain_string_in_an_array() {
    let list = r#" [ "2025-11-25" , {"x":[1]}, 7, "2026-07-28" ] "#;
    assert_eq!(array_has_string(list, "2026-07-28"), Ok(true));
    assert_eq!(array_has_string(list, "2025-11-25"), Ok(true));
    assert_eq!(array_has_string(list, "2026-07"), Ok(false));
    assert_eq!(
        array_has_string(r#"["2026\u002d07-28"]"#, "2026-07-28"),
        Ok(false)
    );
    assert_eq!(array_has_string("[]", "x"), Ok(false));
    assert_eq!(array_has_string("{}", "x"), Err(DecodeError::NotAnArray));
    assert_eq!(
        array_has_string(r#"["x""#, "x"),
        Err(DecodeError::InvalidJson)
    );
}

#[test]
fn decodes_notifications_and_2025_server_requests() {
    let progress = decode(
        r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":3,"progress":0.5}}"#,
    );
    assert_eq!(
        progress,
        Ok(Message::Notification {
            method: "notifications/progress",
            params: Some(r#"{"progressToken":3,"progress":0.5}"#)
        })
    );
    assert_eq!(
        decode(r#"{"jsonrpc":"2.0","id":"srv-1","method":"ping"}"#),
        Ok(Message::Request {
            id: r#""srv-1""#,
            method: "ping",
            params: None
        })
    );
}

/// `n` arrays inside a result inside the line: `n + 2` levels.
fn nested(n: usize) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"a":{}{}}}}}"#,
        "[".repeat(n),
        "]".repeat(n)
    )
}

#[test]
fn rejects_malformed_and_ambiguous_messages() {
    let deep = nested(5000);
    let cases: [(&str, DecodeError); 19] = [
        ("", DecodeError::InvalidJson),
        (
            r#"[{"jsonrpc":"2.0","id":1,"result":{}}]"#,
            DecodeError::NotAnObject,
        ),
        (r#"{"id":1,"result":{}}"#, DecodeError::NotJsonRpc2),
        (
            r#"{"jsonrpc":"1.0","id":1,"result":{}}"#,
            DecodeError::NotJsonRpc2,
        ),
        (r#"{"jsonrpc":"2.0","id":1}"#, DecodeError::Ambiguous),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":1,"message":"x"}}"#,
            DecodeError::Ambiguous,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"m","result":{}}"#,
            DecodeError::Ambiguous,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"id":2,"result":{}}"#,
            DecodeError::DuplicateField,
        ),
        (
            r#"{"jsonrpc":"2.0","method":"to\u006fls/list"}"#,
            DecodeError::InvalidMethod,
        ),
        (
            r#"{"jsonrpc":"2.0","method":7}"#,
            DecodeError::InvalidMethod,
        ),
        (
            r#"{"jsonrpc":"2.0","method":"m","params":[1]}"#,
            DecodeError::InvalidParams,
        ),
        (
            r#"{"jsonrpc":"2.0","id":null,"method":"m"}"#,
            DecodeError::InvalidId,
        ),
        (
            r#"{"jsonrpc":"2.0","id":{},"result":{}}"#,
            DecodeError::InvalidId,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":[]}"#,
            DecodeError::InvalidResult,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":1.5,"message":"x"}}"#,
            DecodeError::InvalidErrorObject,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"message":"x"}}"#,
            DecodeError::InvalidErrorObject,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":"boom"}"#,
            DecodeError::InvalidErrorObject,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{}} x"#,
            DecodeError::InvalidJson,
        ),
        (&deep, DecodeError::TooDeep),
    ];
    for (input, err) in cases {
        assert_eq!(
            decode(input).err(),
            Some(err),
            "{}",
            &input[..input.len().min(80)]
        );
    }
}

#[test]
fn a_line_decodes_up_to_fxs_depth_and_no_further() {
    // `p5-mcp-discover-stdio-depth`: fx reads 2584 levels and refuses 2585.
    assert!(decode(&nested(2582)).is_ok());
    assert_eq!(decode(&nested(2583)).err(), Some(DecodeError::TooDeep));
    // And a refused line that deep names no request: it is kept aside.
    assert_eq!(response_id(&nested(2583)), None);
    // A duplicated wanted field is refused wherever it is wanted.
    assert_eq!(
        decode(
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"complete","resultType":"complete"}}"#
        )
        .err(),
        Some(DecodeError::DuplicateField)
    );
}

#[test]
fn the_scanner_is_strict_json() {
    for bad in [
        r#"{"a":01}"#,
        r#"{"a":1.}"#,
        r#"{"a":.5}"#,
        r#"{"a":-}"#,
        r#"{"a":1e}"#,
        r#"{"a":[1,]}"#,
        r#"{"a":1,}"#,
        r#"{"a" 1}"#,
        r#"{"a":"\x"}"#,
        r#"{"a":"\ud800"}"#,
        r#"{"a":"\udc00x"}"#,
        "{\"a\":\"\u{1}\"}",
        r#"{"a":tru}"#,
        r#"{a:1}"#,
        r#"{"a":[1}"#,
        r#"{"a":{]}"#,
    ] {
        assert_eq!(
            object_fields(bad, ["a"]).err(),
            Some(DecodeError::InvalidJson),
            "{bad}"
        );
    }
    for good in [
        r#"{"a":-0.5e+10}"#,
        r#"{"a":"\ud83d\ude00"}"#,
        r#"{"a":"😀"}"#,
        r#"{"a":[[],{},"",0,true,false,null]}"#,
        " \t\r\n{\"a\" : 1 } \n",
    ] {
        assert!(object_fields(good, ["a"]).is_ok(), "{good}");
    }
}

/// A small fixed-seed generator.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) % n as u64) as usize
    }
}

#[test]
fn mutated_messages_never_crash_and_slices_stay_inside_the_input() {
    let seeds = [
        r#"{"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"a\"b"}],"resultType":"complete"}}"#,
        r#"{"jsonrpc":"2.0","id":"9","error":{"code":-32602,"message":"bad","data":[1,2]}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":1,"progress":2}}"#,
    ];
    let within = |input: &str, part: &str| {
        let (start, at) = (input.as_ptr() as usize, part.as_ptr() as usize);
        at >= start && at + part.len() <= start + input.len()
    };
    let mut rng = Rng(0xdec0de);
    let mut decoded = 0;
    for _ in 0..20_000 {
        let mut bytes = seeds[rng.below(seeds.len())].as_bytes().to_vec();
        for _ in 0..1 + rng.below(3) {
            match rng.below(3) {
                0 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.below(128) as u8;
                }
                1 => bytes.truncate(rng.below(bytes.len() + 1)),
                _ => {
                    let at = rng.below(bytes.len() + 1);
                    bytes.insert(at, b"{}[]\",:0\\"[rng.below(9)]);
                }
            }
        }
        let Ok(input) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let _ = response_id(input);
        let Ok(message) = decode(input) else {
            continue;
        };
        decoded += 1;
        match message {
            Message::Response(r) => match r.body {
                Body::Result { raw, .. } => assert!(within(input, raw)),
                Body::Failure(f) => {
                    assert!(within(input, f.message));
                    if let Some(data) = f.data {
                        assert!(within(input, data));
                    }
                }
            },
            Message::Notification { method, params } => {
                assert!(within(input, method));
                if let Some(p) = params {
                    assert!(within(input, p));
                }
            }
            Message::Request { id, method, .. } => {
                assert!(within(input, id) && within(input, method));
            }
        }
    }
    assert!(decoded > 100, "{decoded}");
}

#[test]
fn a_refused_reply_fails_with_why_and_the_start_of_the_line_as_it_came() {
    // `onRefusedLine` (`src/mcp/server.zig:772-790`): the line's own bytes,
    // not a re-encoding, at most 80 of them (`excerpt_max`, `:55-56`), never
    // a cut character, a control byte as '?', and "..." when cut
    // (`excerptOf`, `maskedPrefix`, `src/mcp/io/process.zig:170-177`).
    let line = r#"{"jsonrpc":"1.0", "id":1, "result":{}}"#;
    let why = decode(line).expect_err("refused").why();
    assert_eq!(
        invalid_reply(why, line),
        r#"invalid reply (its jsonrpc isn't "2.0"): {"jsonrpc":"1.0", "id":1, "result":{}}"#
    );
    let long = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":"{}"}}"#,
        "x".repeat(100)
    );
    let why = decode(&long).expect_err("refused").why();
    assert_eq!(why, "its result isn't an object");
    assert_eq!(
        invalid_reply(why, &long),
        format!("invalid reply ({why}): {}...", &long[..EXCERPT_MAX])
    );
    // A two-byte character across byte 80 is left out whole; a control byte
    // is shown as '?'.
    let wide = format!("\u{1b}{}éb", "a".repeat(78));
    assert_eq!(wide.len(), 82);
    assert_eq!(
        invalid_reply("it isn't valid JSON-RPC", &wide),
        format!(
            "invalid reply (it isn't valid JSON-RPC): ?{}...",
            "a".repeat(78)
        )
    );
    for (input, why) in [
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":1,"message":"m"}}"#,
            "it has both a result and an error, or neither",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":"1","message":"m"}}"#,
            "its error isn't an error object",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"id":1,"result":{}}"#,
            "it isn't valid JSON-RPC",
        ),
    ] {
        assert_eq!(decode(input).expect_err(input).why(), why, "{input}");
    }
}
