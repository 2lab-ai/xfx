    // T-U4 round 2, MUST 2: what a long non-command argument costs the shell.
    // Appended to `src/tui/shell.rs`'s `mod tests` on a throwaway clone (never
    // committed to src), run one case per process so `ru_maxrss` is that
    // case's peak:
    //
    //   XFX_PROBE=<case> cargo test --release --locked --lib -- --ignored \
    //     --exact tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture
    //
    // Cases: `short` (a 9-byte pattern, the baseline), `plain` (8 MiB of `a`,
    // the stream's bound, `MAX_COMPLETION_BYTES`), `c1` (4 Mi x U+009B: 8 MiB
    // raw, each spelled `\u{009b}`, 32 MiB spelled -- the widest expansion a
    // JSON string carries raw), `marks` (2 Mi x U+202E: 6 MiB raw, 16 MiB
    // spelled).
    //
    // What it times, through `Shell::apply` and the bridge's `made_inert` as a
    // live turn runs them:
    //   bridge   -- `UiEvent::ToolStart(..).made_inert()`, the channel's one
    //               spelling of the argument;
    //   start    -- `apply` of that ToolStart: the group's first block;
    //   rebuild  -- each of 40 later events that redraw the open group (a short
    //               read's start, then its result, 20 times), each followed by
    //               `take_pending`, the rows the frame takes;
    //   ctrl-o   -- opening Ctrl-O and laying its page out (`screen_rows`),
    //               again with the layout cached, and once more after a call
    //               joins the group while it is open (the record changed).
    #[test]
    #[ignore]
    fn probe_render_cost_of_a_long_argument() {
        use std::time::Instant;
        let case = std::env::var("XFX_PROBE").unwrap_or_else(|_| "short".into());
        let pattern = match case.as_str() {
            "plain" => "a".repeat(8 << 20),
            "c1" => "\u{9b}".repeat(4 << 20),
            "marks" => "\u{202e}".repeat(2 << 20),
            _ => "**/*.rsxy".to_string(),
        };
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let mut live = shell(40, 120);
        live.route_bytes(b"go\r");
        let _ = live.picks_up();
        live.apply(UiEvent::TurnStarted.made_inert());
        let _ = live.take_pending();

        let at = Instant::now();
        let event = UiEvent::ToolStart {
            call_id: "g".to_string(),
            tool: "glob_files".to_string(),
            shown: crate::tools::read::GLOB_FILES.shown(&serde_json::json!({ "pattern": pattern })),
        }
        .made_inert();
        let bridge = at.elapsed();
        let at = Instant::now();
        live.apply(event);
        let _ = live.take_pending();
        let start = at.elapsed();

        let mut rebuilds = Vec::new();
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
                let event = event.made_inert();
                let at = Instant::now();
                live.apply(event);
                let _ = live.take_pending();
                rebuilds.push(at.elapsed());
            }
        }
        rebuilds.sort();
        let median = rebuilds[rebuilds.len() / 2];
        let worst = *rebuilds.last().expect("rebuilds");

        let at = Instant::now();
        live.route_bytes(b"\x0f");
        let rows = live.screen_rows();
        let open = at.elapsed();
        let at = Instant::now();
        let again = live.screen_rows();
        let cached = at.elapsed();
        assert_eq!(rows, again);
        // A call that joins the open group while Ctrl-O is up: the record
        // changes, so the page is laid out again.
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
        let changed = at.elapsed();

        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        // macOS reports bytes, Linux kilobytes.
        let peak_mib = if cfg!(target_os = "macos") {
            usage.ru_maxrss as f64 / (1 << 20) as f64
        } else {
            usage.ru_maxrss as f64 / 1024.0
        };
        eprintln!(
            "PROBE case={case} raw_bytes={} bridge_ms={:.1} start_ms={:.1} rebuild_median_ms={:.1} rebuild_worst_ms={:.1} ctrlo_open_ms={:.1} ctrlo_cached_ms={:.1} ctrlo_after_event_ms={:.1} peak_rss_mib={:.0}",
            pattern.len(),
            ms(bridge),
            ms(start),
            ms(median),
            ms(worst),
            ms(open),
            ms(cached),
            ms(changed),
            peak_mib
        );
    }
