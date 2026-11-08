    // GRID-ROW-COST: what a long row costs the **whole frame path** -- the
    // event applied to the shell, then one tick of the loop's writer
    // (`commit_frame`: the document's appends through `Band::append_floating`
    // and its `check::preflight`, the band's frame through its own, each
    // written to a screen that takes everything) -- for one or several long
    // calls in an open tool group, and for a group that keeps receiving
    // updates. Appended to `src/tui/event_loop.rs`'s `mod tests` on a
    // throwaway clone (never committed to src) and run one case a process, so
    // `ru_maxrss` is that case's peak:
    //
    //   XFX_PROBE=<case> [XFX_PROBE_ROWS=<n>] [XFX_PROBE_EVENTS=<n>] \
    //   [XFX_PROBE_DETAIL=1] cargo test --release --locked --lib -- --ignored \
    //     --exact tui::event_loop::tests::probe_frame_cost_of_long_rows --nocapture
    //
    // Cases (train 6's and T-U4's): `short` (a 9-byte pattern), `heavy` (one
    // letter under 4 Mi U+0301, then `tail`: one cluster of 8 MiB that paints
    // one cell), `zw` (2.8 Mi x U+180E: 8 MiB of clusters that paint nothing),
    // `c1` (4 Mi x U+009B, spelled), `wider` (2 Mi x U+261D). Each of the
    // `XFX_PROBE_ROWS` long calls (default 1) gets its own pattern: the case's
    // with the call's number in front, so no two rows are one text.
    //
    // What it times, per event, in ms:
    //   apply -- `Shell::apply` of the event;
    //   frame -- the tick after it: every append the event owed, through its
    //            check, and the band's frame, through its own;
    // for each long call's start (`start`), then for `XFX_PROBE_EVENTS`
    // (default 20) short reads joining the group, a start and a result each
    // (`event`: median, 90th percentile, worst of apply + frame), and the bytes
    // the ticks wrote.
    #[test]
    #[ignore]
    fn probe_frame_cost_of_long_rows() {
        let case = std::env::var("XFX_PROBE").unwrap_or_else(|_| "short".into());
        let long: usize = std::env::var("XFX_PROBE_ROWS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(1);
        let events: usize = std::env::var("XFX_PROBE_EVENTS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(20);
        let detail = std::env::var_os("XFX_PROBE_DETAIL").is_some();
        let pattern = |k: usize| -> String {
            let body = match case.as_str() {
                "heavy" => format!("a{}tail", "\u{301}".repeat(4 << 20)),
                "zw" => "\u{180e}".repeat((8 << 20) / 3),
                "c1" => "\u{9b}".repeat(4 << 20),
                "wider" => "\u{261d}".repeat(2 << 20),
                _ => "**/*.rsxy".to_string(),
            };
            format!("{k}{body}")
        };
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let mut shell = shell_of(40, 120);
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        tick(&mut shell, &mut band, &mut out, &mut failures, now);
        shell.route_bytes(b"go\r");
        shell.apply(UiEvent::TurnStarted.made_inert());
        tick(&mut shell, &mut band, &mut out, &mut failures, now);

        let mut starts = Vec::new();
        let mut start_bytes = 0usize;
        for k in 0..long {
            let value = pattern(k);
            let mut owed = vec![UiEvent::ToolStart {
                call_id: format!("g{k}"),
                tool: "glob_files".to_string(),
                shown: crate::tools::read::GLOB_FILES
                    .shown(&serde_json::json!({ "pattern": value.clone() })),
            }];
            if detail {
                owed.push(UiEvent::ToolDetail {
                    call_id: format!("g{k}"),
                    detail: crate::tui::full_transcript::Detail::Arguments(vec![
                        crate::tui::full_transcript::Argument {
                            name: "pattern".to_string(),
                            value,
                            text: true,
                        },
                    ]),
                });
            }
            for event in owed {
                let event = event.made_inert();
                let at = Instant::now();
                shell.apply(event);
                out.written.clear();
                commit_frame(&mut shell, &mut band, &mut out, &mut failures, now, Reconciled)
                    .expect("a tick");
                starts.push(at.elapsed());
                start_bytes += out.written.len();
            }
        }

        let mut applies = Vec::new();
        let mut frames = Vec::new();
        let mut totals = Vec::new();
        let mut bytes = Vec::new();
        for i in 0..events {
            let id = format!("r{i}");
            for event in [
                UiEvent::ToolStart {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    shown: crate::tools::read::READ_FILE
                        .shown(&serde_json::json!({ "path": "a.txt" })),
                },
                UiEvent::ToolResult {
                    call_id: id.clone(),
                    tool: "read_file".to_string(),
                    ok: true,
                    detail: String::new(),
                    outcome: crate::tools::Outcome::Finished,
                    changes: None,
                },
            ] {
                let event = event.made_inert();
                let at = Instant::now();
                shell.apply(event);
                let applied = at.elapsed();
                out.written.clear();
                let at = Instant::now();
                commit_frame(&mut shell, &mut band, &mut out, &mut failures, now, Reconciled)
                    .expect("a tick");
                let framed = at.elapsed();
                applies.push(applied);
                frames.push(framed);
                totals.push(applied + framed);
                bytes.push(out.written.len());
            }
        }
        let stat = |values: &mut Vec<std::time::Duration>| {
            values.sort();
            let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
            (ms(at(0.5)), ms(at(0.9)), ms(*values.last().expect("events")))
        };
        let (apply_median, _, apply_worst) = stat(&mut applies);
        let (frame_median, frame_p90, frame_worst) = stat(&mut frames);
        let (event_median, event_p90, event_worst) = stat(&mut totals);
        let start_worst = starts.iter().max().map_or(0.0, |d| ms(*d));
        bytes.sort_unstable();
        let bytes_median = bytes[bytes.len() / 2];
        let bytes_worst = *bytes.last().expect("events");

        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        // macOS reports bytes, Linux kilobytes.
        let peak_mib = if cfg!(target_os = "macos") {
            usage.ru_maxrss as f64 / (1 << 20) as f64
        } else {
            usage.ru_maxrss as f64 / 1024.0
        };
        eprintln!(
            "FRAMEPROBE case={case} rows={long} events={} detail={detail} raw_bytes={} start_worst_ms={start_worst:.3} start_bytes={start_bytes} apply_median_ms={apply_median:.3} apply_worst_ms={apply_worst:.3} frame_median_ms={frame_median:.3} frame_p90_ms={frame_p90:.3} frame_worst_ms={frame_worst:.3} event_median_ms={event_median:.3} event_p90_ms={event_p90:.3} event_worst_ms={event_worst:.3} bytes_median={bytes_median} bytes_worst={bytes_worst} peak_rss_mib={peak_mib:.0}",
            events * 2,
            pattern(0).len(),
        );
    }
