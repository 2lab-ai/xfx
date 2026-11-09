
    // -----------------------------------------------------------------------
    // P5 train 11's join checks
    // -----------------------------------------------------------------------

    #[test]
    fn a_tall_paste_with_an_enter_behind_its_marker_previews_nothing_and_an_enter_after_it_once() {
        // P5 train 11, join check 1 (SUBMIT-CARD-PREVIEW x PASTE-GAP): what
        // one Enter does. Covered apart before: PASTE-GAP's
        // `a_readiness_gap_inside_one_paste_does_not_turn_the_rest_of_it_into_keys`
        // and PASTE-AFTER-END's carry-over test send nothing for an Enter
        // behind a paste's end marker (the composer and the work channel, no
        // frame); SUBMIT-CARD-PREVIEW's
        // `the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame`
        // previews a pasted draft whose Enter is typed apart (`route_bytes`:
        // a whole delivery and its quiet). Neither sends a paste taller than
        // the room through the delivery the quarantine reads. Here, on a full
        // screen, one delivery holds a twelve-line paste and an Enter behind
        // its marker, read by the loop's own reads (`burst`): nothing is
        // sent, nothing previewed, fx's warning written. Once the input has
        // been quiet, an Enter of its own sends the draft once, and the tick
        // after it writes one preview -- the newest rows only -- then the
        // card, once, in order.
        let words: Vec<String> = (1..=12).map(|n| format!("GAP-ROW-{n:02}")).collect();
        let mut shell = shell_of(40, 120);
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let mut terminal = Terminal::new(40, 120);
        shell.apply(UiEvent::Delta("FILLER\n\n".repeat(30)));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.flush_paced();
        for _ in 0..2 {
            loop_tick(
                &mut shell,
                &mut band,
                &mut failures,
                &mut out,
                &mut terminal,
            );
        }

        let delivery = Readable {
            bytes: format!("\x1b[200~{}\x1b[201~\r", words.join("\n")).into_bytes(),
            at: std::cell::Cell::new(0),
        };
        delivery.turn(&mut shell);
        shell.settle_input(Instant::now());
        shell.settle_band(Instant::now());
        assert!(
            !shell.previewing(),
            "the Enter behind the marker started a preview"
        );
        loop_tick(
            &mut shell,
            &mut band,
            &mut failures,
            &mut out,
            &mut terminal,
        );
        assert!(
            shell._work.try_recv().is_err(),
            "the Enter behind the marker sent the draft"
        );
        assert!(
            !shell.previewing(),
            "the tick after the quarantined Enter is a preview's"
        );
        assert!(
            terminal
                .everything()
                .iter()
                .any(|row| row.contains("Input that arrived right after the paste was ignored.")),
            "the warning was not written: {:#?}",
            terminal.everything()
        );

        // The input goes quiet: an idle turn's check, and one a quiet period
        // later, end the quarantine; an Enter of its own is a key again.
        let idle = Readable {
            bytes: Vec::new(),
            at: std::cell::Cell::new(0),
        };
        idle.turn(&mut shell);
        shell.end_delivery_at(Instant::now() + super::super::input::Decoder::PASTE_QUIET);
        loop_tick(
            &mut shell,
            &mut band,
            &mut failures,
            &mut out,
            &mut terminal,
        );
        assert!(shell._work.try_recv().is_err(), "the quiet sent the draft");
        out.written.clear();
        terminal.fed = 0;
        let enter = Readable {
            bytes: b"\r".to_vec(),
            at: std::cell::Cell::new(0),
        };
        enter.turn(&mut shell);
        shell.settle_input(Instant::now());
        shell.settle_band(Instant::now());
        assert!(
            shell.previewing(),
            "the Enter of its own started no preview"
        );
        loop_tick(
            &mut shell,
            &mut band,
            &mut failures,
            &mut out,
            &mut terminal,
        );
        assert!(
            matches!(
                shell._work.try_recv(),
                Ok(crate::tui::bridge::TurnWork::Submit(prompt)) if prompt == words.join("\n")
            ),
            "the Enter of its own did not send the pasted draft"
        );
        assert!(shell._work.try_recv().is_err(), "the draft was sent twice");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        let updates: Vec<&str> = written.split("\u{1b}[?2026h").skip(1).collect();
        let preview =
            |update: &str| update.contains("GAP-ROW-12") && !update.contains("GAP-ROW-01");
        assert!(
            updates.len() >= 2 && preview(updates[0]),
            "the tick's first update is not the preview: {written:?}"
        );
        assert_eq!(
            updates.iter().filter(|update| preview(update)).count(),
            1,
            "not exactly one preview: {written:?}"
        );
        card_once(&terminal, &words, "the pasted card");
        assert!(
            !shell.owes_document(),
            "the card is still owed after the tick"
        );
        let before = out.written.len();
        loop_tick(
            &mut shell,
            &mut band,
            &mut failures,
            &mut out,
            &mut terminal,
        );
        assert!(
            !String::from_utf8_lossy(&out.written[before..]).contains("GAP-ROW-"),
            "the next tick wrote the card's rows again"
        );
    }
