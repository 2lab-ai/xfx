    // GRID-ROW-COST, item 5: where a long argument's memory is, stage by stage.
    // Train 6's probe (`../train-6/probe/render_cost_probe.rs`, DETAILS=1: the
    // call's start, then its arguments) cut to its events, with the heap the
    // process holds (macOS `malloc_zone_statistics`, every zone: bytes in use)
    // and its peak resident set (`ru_maxrss`) printed after each stage, so the
    // stage a residual is made in, and whether it is held or only passed
    // through, can be read off. Appended to `src/tui/shell.rs`'s `mod tests` on
    // a throwaway clone (never committed to src); macOS only; one case a
    // process:
    //
    //   XFX_PROBE=<case> cargo test --release --locked --lib -- --ignored \
    //     --exact tui::shell::tests::probe_memory_of_a_long_argument --nocapture
    //
    // Cases: `wider` (2 Mi x U+261D, train 6's residual: main 84 MiB peak, the
    // train 138), `heavy` (one letter under 4 Mi U+0301, then `tail`), `short`.
    #[test]
    #[ignore]
    fn probe_memory_of_a_long_argument() {
        #[repr(C)]
        #[derive(Default)]
        struct MallocStatistics {
            blocks_in_use: u32,
            size_in_use: usize,
            max_size_in_use: usize,
            size_allocated: usize,
        }
        extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut MallocStatistics);
        }
        let stage = |name: &str| {
            let mut stats = MallocStatistics::default();
            unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
            eprintln!(
                "MEM stage={name} live_mib={:.1} heap_peak_mib={:.1} peak_rss_mib={:.1}",
                stats.size_in_use as f64 / (1 << 20) as f64,
                stats.max_size_in_use as f64 / (1 << 20) as f64,
                usage.ru_maxrss as f64 / (1 << 20) as f64,
            );
            // `XFX_PROBE_PAUSE_AT=<stage>`: hold the process there for a heap
            // tool (`malloc_history <pid> -allBySize` under
            // `MallocStackLogging=1`) to read what is live.
            if std::env::var("XFX_PROBE_PAUSE_AT").is_ok_and(|at| at == name) {
                eprintln!("PAUSED pid={}", std::process::id());
                std::thread::sleep(std::time::Duration::from_secs(90));
            }
        };
        let case = std::env::var("XFX_PROBE").unwrap_or_else(|_| "short".into());
        let pattern = match case.as_str() {
            "wider" => "\u{261d}".repeat(2 << 20),
            "heavy" => format!("a{}tail", "\u{301}".repeat(4 << 20)),
            _ => "**/*.rsxy".to_string(),
        };
        stage("pattern");
        let mut live = shell(40, 120);
        live.route_bytes(b"go\r");
        let _ = live.picks_up();
        live.apply(UiEvent::TurnStarted.made_inert());
        let _ = live.take_pending();
        stage("shell");
        let event = UiEvent::ToolStart {
            call_id: "g".to_string(),
            tool: "glob_files".to_string(),
            shown: crate::tools::read::GLOB_FILES.shown(&serde_json::json!({ "pattern": pattern })),
        }
        .made_inert();
        live.apply(event);
        let _ = live.take_pending();
        stage("start");
        live.apply(
            UiEvent::ToolDetail {
                call_id: "g".to_string(),
                detail: full_transcript::Detail::Arguments(vec![full_transcript::Argument {
                    name: "pattern".to_string(),
                    value: pattern.clone(),
                    text: true,
                }]),
            }
            .made_inert(),
        );
        let _ = live.take_pending();
        stage("detail");
        for i in 0..20 {
            let id = format!("r{i}");
            for event in [
                UiEvent::ToolStart {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({ "path": "a.txt" })),
                },
                UiEvent::ToolResult {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    ok: true,
                    detail: String::new(),
                    outcome: CallOutcome::Finished,
                    changes: None,
                },
            ] {
                live.apply(event.made_inert());
                let _ = live.take_pending();
            }
        }
        stage("rebuilds");
        live.route_bytes(b"\x0f");
        let _ = live.screen_rows();
        stage("ctrlo_open");
        live.route_bytes(b"\x1b[5~");
        let _ = live.screen_rows();
        stage("ctrlo_pageup");
        live.apply(
            UiEvent::ToolStart {
                call_id: "late".to_string(),
                tool: "read_file".to_string(),
                shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({ "path": "b.txt" })),
            }
            .made_inert(),
        );
        let _ = live.screen_rows();
        stage("ctrlo_join");
        live.route_bytes(b"\x0f");
        stage("ctrlo_closed");
        live.route_bytes(b"\x1b[?997;2n");
        live.route_bytes(b"\x0f");
        let _ = live.screen_rows();
        stage("ctrlo_theme");
        let _ = live.resize(40, 100);
        let _ = live.screen_rows();
        stage("ctrlo_resize");
        live.route_bytes(b"\x0f");
        let _ = live.screen_rows();
        stage("ctrlo_closed_again");
        drop(live);
        stage("dropped");
    }
