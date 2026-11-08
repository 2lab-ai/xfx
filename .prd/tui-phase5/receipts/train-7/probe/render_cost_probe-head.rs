    // Train 7, join check 6: train 6's probe for a head where the start
    // names its submission (STOP-NEXT): `live.start_turn()` where it applied
    // `UiEvent::TurnStarted.made_inert()`, nothing else changed.
    // Train 6, join check 2: T-U4's round 3 probe
    // (`.prd/tui-phase5/receipts/cancel-marker/round3/render_cost_probe.rs`),
    // with two things added and nothing else changed:
    //   - a case `wider`: 2 Mi x U+261D, a scalar a terminal draws wider than
    //     xfx and fx count it, which CTRLO-WIDER names on Ctrl-O's tool lines
    //     (`\u{261d}`, 8 bytes, where the raw scalar is 3);
    //   - `XFX_PROBE_DETAIL=1`: after the call's start, the call's arguments
    //     (`UiEvent::ToolDetail`, `Detail::Arguments`, through the channel's
    //     `made_inert`) -- what a live turn's journal hands Ctrl-O for every
    //     call (`full_transcript::details`), and what T-U4's probe never sent,
    //     so its Ctrl-O laid out a heading and no argument row. Timed as
    //     `detail_ms` (the apply and `take_pending`).
    // T-U4 round 3: what a long non-command argument costs the shell, the
    // transcript's row and Ctrl-O. Round 2's probe (`../round2/`) with two
    // byte-heavy cases and Ctrl-O's other events. Appended to
    // `src/tui/shell.rs`'s `mod tests` on a throwaway clone (never committed
    // to src), run one case per process so `ru_maxrss` is that case's peak:
    //
    //   XFX_PROBE=<case> cargo test --release --locked --lib -- --ignored \
    //     --exact tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture
    //
    // Cases: `short` (a 9-byte pattern), `plain` (8 MiB of `a`, the stream's
    // bound), `c1` (4 Mi x U+009B, 32 MiB spelled), `marks` (2 Mi x U+202E,
    // 16 MiB spelled), `heavy` (one letter under 4 Mi combining marks, then
    // `tail`: one cluster of 8 MiB that paints one cell), `zw` (2.8 Mi x
    // U+180E: 8 MiB of clusters that paint nothing).
    //
    // What it times, through `Shell::apply` and the bridge's `made_inert` as a
    // live turn runs them:
    //   bridge   -- the channel's one spelling of the argument;
    //   start    -- `apply` of the ToolStart and `take_pending`: the group's
    //               first block, cut for the screen;
    //   rebuild  -- each of 40 later events that redraw the open group (a short
    //               read's start, then its result, 20 times), each with
    //               `take_pending`, the rows the frame takes;
    //   grid     -- the frame's own work on the call's row as the last
    //               rebuild wrote it: `Grid::row_matches` (what an append that
    //               leaves the row in place pays) and `Grid::place_row` (what
    //               one that moves it pays, before its bytes are written);
    //   ctrl-o   -- Ctrl-O opened and its page laid out (`screen_rows`), again
    //               cached, a PageUp and the page after it, a call that joins
    //               the group while it is open, a theme switch (closed,
    //               reported light, opened), and a resize to 100 columns.
    #[test]
    #[ignore]
    fn probe_render_cost_of_a_long_argument() {
        use std::time::Instant;
        let case = std::env::var("XFX_PROBE").unwrap_or_else(|_| "short".into());
        let pattern = match case.as_str() {
            "plain" => "a".repeat(8 << 20),
            "c1" => "\u{9b}".repeat(4 << 20),
            "marks" => "\u{202e}".repeat(2 << 20),
            "heavy" => format!("a{}tail", "\u{301}".repeat(4 << 20)),
            "zw" => "\u{180e}".repeat((8 << 20) / 3),
            "wider" => "\u{261d}".repeat(2 << 20),
            _ => "**/*.rsxy".to_string(),
        };
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let mut live = shell(40, 120);
        live.route_bytes(b"go\r");
        let _ = live.picks_up();
        live.start_turn();
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
        let mut detail = std::time::Duration::ZERO;
        if std::env::var_os("XFX_PROBE_DETAIL").is_some() {
            let at = Instant::now();
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
            detail = at.elapsed();
        }

        let mut rebuilds = Vec::new();
        let mut last = Vec::new();
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
                let appends = live.take_pending();
                rebuilds.push(at.elapsed());
                if !appends.is_empty() {
                    last = appends;
                }
            }
        }
        rebuilds.sort();
        let median = rebuilds[rebuilds.len() / 2];
        let worst = *rebuilds.last().expect("rebuilds");

        // The call's row as the transcript last wrote it: the longest row.
        let row = last
            .iter()
            .flat_map(|append| append.rows.iter())
            .max_by_key(|row| row.len())
            .cloned()
            .unwrap_or_default();
        let geometry = crate::tui::layout::solve(40, 120, 1).expect("a band");
        let mut grid = super::super::grid::Grid::blank(geometry.rows, geometry.cols);
        let mut scratch = super::super::grid::Grid::blank(geometry.rows, geometry.cols);
        let at = Instant::now();
        grid.place_row(2, &row, &geometry);
        let place = at.elapsed();
        let at = Instant::now();
        let matched = grid.row_matches(2, &row, &geometry, &mut scratch);
        let matches = at.elapsed();
        assert!(matched);

        let at = Instant::now();
        live.route_bytes(b"\x0f");
        let rows = live.screen_rows();
        let open = at.elapsed();
        let at = Instant::now();
        let again = live.screen_rows();
        let cached = at.elapsed();
        assert_eq!(rows, again);
        let at = Instant::now();
        live.route_bytes(b"\x1b[5~");
        let _ = live.screen_rows();
        let page_up = at.elapsed();
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
        live.route_bytes(b"\x0f");
        live.route_bytes(b"\x1b[?997;2n");
        let at = Instant::now();
        live.route_bytes(b"\x0f");
        let _ = live.screen_rows();
        let themed = at.elapsed();
        let at = Instant::now();
        let _ = live.resize(40, 100);
        let _ = live.screen_rows();
        let resized = at.elapsed();

        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        // macOS reports bytes, Linux kilobytes.
        let peak_mib = if cfg!(target_os = "macos") {
            usage.ru_maxrss as f64 / (1 << 20) as f64
        } else {
            usage.ru_maxrss as f64 / 1024.0
        };
        eprintln!(
            "PROBE case={case} detail={} raw_bytes={} row_bytes={} bridge_ms={:.1} start_ms={:.1} detail_ms={:.1} rebuild_median_ms={:.1} rebuild_worst_ms={:.1} grid_matches_ms={:.1} grid_place_ms={:.1} ctrlo_open_ms={:.1} ctrlo_cached_ms={:.1} ctrlo_pageup_ms={:.1} ctrlo_join_ms={:.1} ctrlo_theme_ms={:.1} ctrlo_resize_ms={:.1} peak_rss_mib={:.0}",
            std::env::var_os("XFX_PROBE_DETAIL").is_some(),
            pattern.len(),
            row.len(),
            ms(bridge),
            ms(start),
            ms(detail),
            ms(median),
            ms(worst),
            ms(matches),
            ms(place),
            ms(open),
            ms(cached),
            ms(page_up),
            ms(joined),
            ms(themed),
            ms(resized),
            peak_mib
        );
    }
