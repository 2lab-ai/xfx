
    #[test]
    fn a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter() {
        // Train 10, join check 3 (AT-HEIGHT x QUESTIONS-20). AT-HEIGHT's `@`
        // list takes Tab, the arrows and Enter (`offer_to_files`), and a
        // twenty-question panel binds the same keys -- Tab is its next page
        // (`Question 10 of 20` after nine). The panel is asked before
        // `offer_to_files` (`Self::answer`, first in `consume`'s chain after
        // the full transcript), and its arrival dismisses the list
        // (`ask_question` -> `dismiss_picker`), so no key of the batch can
        // reach the list, and the answer does not put it back.
        let (_workspace, mut shell) = in_ws2(40, 120);
        let started = turn_running(&mut shell, b"ask me\r");
        assert_eq!(shell.picks_up(), TurnWork::Submit("ask me".to_string()));
        shell.route_bytes(b"@");
        listed(&mut shell);
        assert!(
            shell
                .file_picker
                .as_ref()
                .is_some_and(|files| files.chosen().is_some()),
            "the list had no row a Tab or an Enter would take"
        );
        shell.apply(UiEvent::Question(QuestionRequest {
            id: QuestionId(20),
            entries: (1..=20)
                .map(|n| QuestionEntry {
                    question: format!("Pick {n}?"),
                    options: vec![choice(&format!("Keep {n}"), None), choice("Skip", None)],
                })
                .collect(),
        }));
        shell.settle_band(started);
        assert!(
            shell.file_picker.is_none(),
            "the list stayed up behind the panel"
        );
        let page = |shell: &Shell, n: usize| {
            shell
                .band_rows()
                .iter()
                .any(|row| visible_text(row).contains(&format!("Question {n} of 20")))
        };
        assert!(page(&shell, 1), "{:?}", shell.band_rows());

        // Tab nine times walks the panel to its tenth page.
        shell.route_bytes(&b"\t".repeat(9));
        assert!(page(&shell, 10), "{:?}", shell.band_rows());
        // Down and Enter answer question ten with its second choice.
        shell.route_bytes(b"\x1b[B");
        shell.route_bytes(b"\r");
        assert!(page(&shell, 11), "{:?}", shell.band_rows());
        assert_eq!(
            shell.editor.text(),
            "@",
            "a key of the batch reached the composer"
        );
        assert!(shell.file_picker.is_none(), "the answer put the list back");
        assert!(
            shell.sent.try_recv().is_err(),
            "a key of the batch sent the draft"
        );
        assert_eq!(
            shell.controlled(),
            None,
            "the batch went before its twenty answers"
        );
    }
