    #[test]
    fn a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays() {
        // Train 8, join check 1 (GRID-ROW-COST x STOP-NEXT-2). GRID-ROW-COST:
        // the frames keep, for each row they place, the text it was laid
        // from, and answer for a row left in place from that record, which
        // holds only while every change to a row's cells forgets its text.
        // STOP-NEXT-2: after a Ctrl-C the stopped turn's text is held for its
        // end, and a tool row read behind held text is held with it
        // (`Shell::late`), then written -- with the stop's own end, with a
        // finish (the text too), or when a drain empties before the end.
        // Here the stop comes over an open group of two long calls (one
        // letter under 64 Ki marks each, as GRID-ROW-COST's session case has
        // them); behind it, late text, a read joining the group and the
        // first long call's result. A tick while they are held (a drain its
        // bound cut short) leaves the group as it was; the ticks after they
        // are written show the read and the result. After every tick: no
        // vector refused, and every row the shadow says was laid from a text
        // holds a fresh placement of it (`Grid::stale_laid_rows`).
        let marks = "\u{301}".repeat(64 * 1024);
        let mut got = Vec::new();
        for case in [
            "the stop's own end",
            "a finished end",
            "a drain that empties before the end",
        ] {
            let mut shell = shell_of(30, 100);
            let mut band = Band::new();
            let mut failures = FrameFailures::default();
            let mut out = screen();
            let now = Instant::now();
            let held = |shell: &Fixture, band: &Band, failures: &FrameFailures, step: &str| {
                assert!(
                    failures.first.is_none(),
                    "{case}, {step}: a vector was refused: {:?}",
                    failures.first
                );
                assert_eq!(
                    band.shadow().stale_laid_rows(&shell.geometry),
                    Vec::<u16>::new(),
                    "{case}, {step}: rows that are not what their text lays"
                );
            };
            let read = |id: &str, path: &str| UiEvent::ToolStart {
                call_id: id.to_string(),
                tool: "read_file".to_string(),
                shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({ "path": path })),
            };
            let result = |id: &str, tool: &str| UiEvent::ToolResult {
                call_id: id.to_string(),
                tool: tool.to_string(),
                ok: true,
                detail: String::new(),
                outcome: crate::tools::Outcome::Finished,
                changes: None,
            };
            tick(&mut shell, &mut band, &mut out, &mut failures, now);
            shell.route_bytes(b"go\r");
            shell.start_turn();
            tick(&mut shell, &mut band, &mut out, &mut failures, now);
            for k in 0..2 {
                shell.apply(
                    UiEvent::ToolStart {
                        call_id: format!("g{k}"),
                        tool: "glob_files".to_string(),
                        shown: crate::tools::read::GLOB_FILES
                            .shown(&serde_json::json!({ "pattern": format!("{k}a{marks}") })),
                    }
                    .made_inert(),
                );
                tick(&mut shell, &mut band, &mut out, &mut failures, now);
                held(&shell, &band, &failures, "a long call started");
            }
            for k in 0..3 {
                let id = format!("r{k}");
                for event in [read(&id, "early.txt"), result(&id, "read_file")] {
                    shell.apply(event.made_inert());
                    tick(&mut shell, &mut band, &mut out, &mut failures, now);
                    held(&shell, &band, &failures, "a read joined");
                }
            }
            // The case's premise: the long rows are on the screen, laid from
            // their text, and the appends above left them where they are.
            let premise = band.shadow().longest_laid() > marks.len();
            // The stop, over the open group.
            shell.route_bytes(&[0x03]);
            tick(&mut shell, &mut band, &mut out, &mut failures, now);
            held(&shell, &band, &failures, "the stop");
            // Behind it: late text, a read joining, the first long call's
            // result -- in a drain its bound cut short, so held.
            shell.apply(UiEvent::Delta("LATE-TEXT\n".to_string()).made_inert());
            shell.apply(read("late", "late.txt").made_inert());
            shell.apply(result("g0", "glob_files").made_inert());
            shell.ui_events_taken(false);
            let holding = tick(&mut shell, &mut band, &mut out, &mut failures, now);
            held(&shell, &band, &failures, "the rows held");
            let mut after = String::new();
            if case == "a drain that empties before the end" {
                shell.ui_events_taken(true);
                after.push_str(&tick(&mut shell, &mut band, &mut out, &mut failures, now));
                held(&shell, &band, &failures, "a drain emptied");
            }
            let end = (case == "the stop's own end")
                .then(|| super::super::bridge::TURN_CANCELLED.to_string());
            shell.apply(UiEvent::TurnEnded { failure: end }.made_inert());
            shell.ui_events_taken(true);
            shell.flush_paced();
            for step in ["the end", "the next tick"] {
                after.push_str(&tick(&mut shell, &mut band, &mut out, &mut failures, now));
                held(&shell, &band, &failures, step);
            }
            // The second long call, still running when the turn ended, was
            // stopped with it; both long rows were written again, and the
            // frames after them know them by their text.
            let known_after = band.shadow().longest_laid() > marks.len();
            got.push((
                case,
                premise,
                holding.contains("late.txt"),
                holding.contains("LATE-TEXT"),
                after.contains("late.txt"),
                after.contains("LATE-TEXT"),
                known_after,
            ));
        }
        assert_eq!(
            got,
            vec![
                ("the stop's own end", true, false, false, true, false, true),
                ("a finished end", true, false, false, true, true, true),
                (
                    "a drain that empties before the end",
                    true,
                    false,
                    false,
                    true,
                    false,
                    true
                ),
            ],
            "each: the long rows laid from their text before the stop; nothing held written \
             while held; then the held read written, the late text only with a finish, and \
             the long rows known by their text again"
        );
    }
