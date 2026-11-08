    // ARGROW-CACHE: what a join, a theme switch and the open cost when every
    // member of the group carries a result at its bound (RESULT_BYTES, 64 KiB
    // of code lines) -- rows `tool_rows` still lays out on every layout of an
    // entry. Twenty reads, one joins while Ctrl-O is open.
    #[test]
    #[ignore]
    fn probe_results_join_cost() {
        use std::time::Instant;
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let mut live = shell(40, 120);
        live.route_bytes(b"go\r");
        let _ = live.picks_up();
        live.apply(UiEvent::TurnStarted.made_inert());
        let line = "    let value = compute_something(alpha, beta, gamma); // a line of code\n";
        let body: String = line.repeat((64 * 1024) / line.len());
        for i in 0..20 {
            let id = format!("r{i}");
            live.apply(
                UiEvent::ToolStart {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({ "path": "a.rs" })),
                }
                .made_inert(),
            );
            live.apply(
                UiEvent::ToolResult {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    ok: true,
                    detail: String::new(),
                    outcome: CallOutcome::Finished,
                    changes: None,
                }
                .made_inert(),
            );
            live.apply(
                UiEvent::ToolDetail {
                    call_id: id.clone(),
                    detail: full_transcript::Detail::Result(body.clone()),
                }
                .made_inert(),
            );
        }
        let _ = live.take_pending();
        let at = Instant::now();
        live.route_bytes(b"\x0f");
        let _ = live.screen_rows();
        let open = at.elapsed();
        let at = Instant::now();
        live.apply(
            UiEvent::ToolStart {
                call_id: "late".to_string(),
                tool: "read_file".to_string(),
                shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({ "path": "b.txt" })),
            }
            .made_inert(),
        );
        let _ = live.screen_rows();
        let joined = at.elapsed();
        live.route_bytes(b"\x1b[?997;2n");
        let at = Instant::now();
        let _ = live.screen_rows();
        let themed = at.elapsed();
        eprintln!(
            "RESULTS calls=20 result_bytes={} open_ms={:.1} join_ms={:.1} theme_ms={:.1}",
            body.len(),
            ms(open),
            ms(joined),
            ms(themed)
        );
    }
