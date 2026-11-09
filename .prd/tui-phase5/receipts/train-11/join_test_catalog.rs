
    /// What `shell` has asked the runtime for the catalog since the last
    /// call, in order.
    fn catalog_asks_sent(shell: &mut Fixture) -> Vec<crate::tui::bridge::CatalogAsk> {
        let mut asks = Vec::new();
        while let Ok(message) = shell._control.try_recv() {
            if let crate::tui::bridge::TurnControl::Catalog(ask) = message {
                asks.push(ask);
            }
        }
        asks
    }

    #[test]
    fn a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one() {
        // P5 train 11, join check 2 (SUBMIT-CARD-PREVIEW x CATALOG-REFRESH).
        // A submitted prompt is catalog activity (`Shell::send` ->
        // `note_catalog_activity`, on the catalog's clock), and the same
        // Enter starts the preview, whose frame and the card's the tick after
        // it writes (`commit_counted`, whose `settle_column` asks only for an
        // open `/model` column). Covered apart before: the ask by
        // `a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing`
        // (`shell.rs`, a one-line prompt and no frame), the preview by
        // `the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame`
        // (no catalog told). Here one Enter does both: a catalog past its
        // failure's hold is asked once, by the Enter, and by nothing the
        // preview's tick or the next one does; inside the hold, or with no
        // catalog told, nothing is asked; and the bytes the tick writes --
        // the preview's update, then the card's -- are the same in all three,
        // the first being SUBMIT-CARD-PREVIEW's own case.
        let words: Vec<String> = ["CATALOG-ONE", "CATALOG-TWO", "CATALOG-THREE"]
            .map(String::from)
            .to_vec();
        let failed = |retry_at| UiEvent::CatalogUnread {
            provider: crate::provider::ProviderId::Gateway,
            retry: Some(crate::provider::model::CatalogRetry::Unreachable),
            retry_at,
        };
        let cases = [
            ("no catalog told", None, 0),
            (
                "inside the failure's hold",
                Some(failed(catalog_due(Duration::from_secs(60)))),
                0,
            ),
            (
                "past the failure's hold",
                Some(failed(catalog_due_ago(Duration::from_secs(60)))),
                1,
            ),
        ];
        let mut ticks = Vec::new();
        for (what, told, owed) in cases {
            let (mut shell, mut band, mut failures, mut out, mut terminal) =
                full_with_draft(&words);
            if let Some(event) = told {
                shell.apply(event);
            }
            loop_tick(
                &mut shell,
                &mut band,
                &mut failures,
                &mut out,
                &mut terminal,
            );
            assert_eq!(
                catalog_asks_sent(&mut shell),
                [],
                "{what}: asked before the Enter"
            );
            out.written.clear();
            terminal.fed = 0;
            shell.route_bytes(b"\r");
            assert!(
                shell.previewing(),
                "{what}: no preview, so this case proves nothing"
            );
            assert_eq!(
                catalog_asks_sent(&mut shell),
                vec![crate::tui::bridge::CatalogAsk::ACTIVITY; owed],
                "{what}: the Enter's asks"
            );
            loop_tick(
                &mut shell,
                &mut band,
                &mut failures,
                &mut out,
                &mut terminal,
            );
            let written = out.written.clone();
            assert_eq!(
                catalog_asks_sent(&mut shell),
                [],
                "{what}: the preview's tick asked"
            );
            loop_tick(
                &mut shell,
                &mut band,
                &mut failures,
                &mut out,
                &mut terminal,
            );
            assert_eq!(
                catalog_asks_sent(&mut shell),
                [],
                "{what}: the tick after the card's asked"
            );
            card_once(&terminal, &words, what);
            assert!(
                matches!(
                    shell._work.try_recv(),
                    Ok(crate::tui::bridge::TurnWork::Submit(prompt)) if prompt == words.join("\n")
                ),
                "{what}: the draft was not sent"
            );
            ticks.push((what, written));
        }
        let unit = String::from_utf8_lossy(&ticks[0].1).into_owned();
        let updates: Vec<&str> = unit.split("\u{1b}[?2026h").skip(1).collect();
        assert!(
            updates.len() >= 2
                && updates[0].contains("CATALOG-THREE")
                && !updates[0].contains("CATALOG-ONE")
                && updates[1..]
                    .iter()
                    .any(|update| update.contains("CATALOG-ONE")),
            "the unit's own case is not a preview, then the card: {unit:?}"
        );
        for (what, written) in &ticks[1..] {
            assert!(
                *written == ticks[0].1,
                "{what}: the tick wrote other bytes than with no catalog told: {:?}",
                String::from_utf8_lossy(written)
            );
        }
    }
