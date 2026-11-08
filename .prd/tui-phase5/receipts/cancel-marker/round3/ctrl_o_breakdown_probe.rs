    // T-U4 round 3, the attribution before the fix: what each step of one
    // call's Ctrl-O layout cost at round 2's code. Appended to
    // `src/tui/full_transcript.rs`'s `mod tests` on a throwaway clone whose
    // full_transcript.rs and Ctrl-O path were 0978e02c's, run in release:
    //
    //   XFX_PROBE=<plain|c1|heavy> cargo test --release --locked --lib -- --ignored \
    //     --exact tui::full_transcript::tests::probe_ctrl_o_breakdown --nocapture
    //
    // A pattern call (already spelled, as the channel hands it over) and 20
    // reads in one group, then each step of `tool_rows` for the pattern's
    // entry and one read's, and the whole record's `measure` at 120 columns.
    #[test]
    #[ignore]
    fn probe_ctrl_o_breakdown() {
        use std::time::Instant;
        let case = std::env::var("XFX_PROBE").unwrap_or_else(|_| "c1".into());
        let pattern = match case.as_str() {
            "plain" => "a".repeat(8 << 20),
            "c1" => "\u{9b}".repeat(4 << 20),
            "marks" => "\u{202e}".repeat(2 << 20),
            "heavy" => format!("a{}tail", "\u{301}".repeat(4 << 20)),
            _ => "x".to_string(),
        };
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let spelled = |shown: Shown| Shown {
            presentation: shown.presentation,
            target: shown.target.map(|t| super::super::toolgroup::spelled(&t, shown.presentation)),
        };
        let mut record = record();
        let at = Instant::now();
        let big = spelled(read::GLOB_FILES.shown(&json!({ "pattern": pattern })));
        eprintln!("spell {:.1}", ms(at.elapsed()));
        record.tool_started("g", &big);
        for i in 0..20 {
            record.tool_started(&format!("r{i}"), &read("a.txt"));
            record.tool_finished(&format!("r{i}"), true, "", Outcome::Finished, None);
        }
        let Body::Tool(call) = &record.entries[0].body else { panic!() };
        let at = Instant::now();
        let mut group = ToolGroup::default();
        group.start(call.id.clone(), call.shown.clone());
        for i in 0..20 { group.start(format!("r{i}"), read("a.txt")); }
        eprintln!("group build {:.1}", ms(at.elapsed()));
        let at = Instant::now();
        let block = group.expanded(DARK);
        eprintln!("expanded {:.1}", ms(at.elapsed()));
        let at = Instant::now();
        let rows = tool_status::expanded_rows(&block[1], 120);
        eprintln!("expanded_rows {:.1} rows={}", ms(at.elapsed()), rows.len());
        let at = Instant::now();
        let heading = block.get(1).cloned().unwrap_or_default();
        eprintln!("heading clone {:.1}", ms(at.elapsed()));
        let at = Instant::now();
        let plain = unpainted(&heading);
        eprintln!("unpainted {:.1} {}", ms(at.elapsed()), plain.len());
        let at = Instant::now();
        let w = wrap::width(&heading);
        eprintln!("width {:.1} {w}", ms(at.elapsed()));
        let at = Instant::now();
        let mut v = Vec::new(); v.extend(rows.iter().cloned());
        eprintln!("rows clone {:.1}", ms(at.elapsed()));
        let at = Instant::now();
        let rows = record.tool_rows(call, 120, DARK);
        eprintln!("tool_rows(big) {:.1} rows={}", ms(at.elapsed()), rows.len());
        let Body::Tool(small) = &record.entries[1].body else { panic!() };
        let at = Instant::now();
        let _ = record.tool_rows(small, 120, DARK);
        eprintln!("tool_rows(small) {:.1}", ms(at.elapsed()));
        let at = Instant::now();
        let (total, _) = record.measure(120, DARK);
        eprintln!("measure {:.1} total={total}", ms(at.elapsed()));
    }
