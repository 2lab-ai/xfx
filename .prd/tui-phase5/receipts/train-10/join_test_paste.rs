
    #[test]
    fn a_paste_into_the_at_word_with_an_enter_behind_its_marker_is_kept_trusted_and_sends_nothing()
    {
        // Train 10, join check 3 (AT-HEIGHT x PASTE-AFTER-END). PASTE-AFTER-END:
        // the bytes behind a paste's end marker in its read are no keys -- the
        // paste is kept and the warning written (`Decoder::feed`'s early
        // return, `input.rs`; fx's `app_input_runtime.zig:9686-9740@481104fd`).
        // AT-HEIGHT: the `@` list's keys act only over rows a frame showed, and
        // a changed query is a new list, trusted again only by the frame that
        // shows it. A paste into the word changes its query; the frame it asks
        // for shows the new rows, and the Enter behind its marker neither takes
        // the marked row nor sends the line.
        let mut list = AtList::open(40, 120);
        assert_eq!(list.keys(&DOWN.repeat(2)), at_window(1, 10, 3));
        let before = list.out.written.len();
        let rows = list.keys(b"\x1b[200~0\x1b[201~\r");
        assert_eq!(
            composer(&list.shell),
            format!("{COMPOSER_MARKER} @f-0"),
            "the paste was lost, or the Enter behind its marker took the marked row"
        );
        assert!(
            list.shell._work.try_recv().is_err(),
            "the Enter behind the end marker sent the line"
        );
        let painted = String::from_utf8_lossy(&list.out.written[before..]).to_string();
        assert!(
            painted.contains("Input that arrived right after the paste was ignored."),
            "no warning"
        );
        assert_eq!(rows, at_window(1, 10, 1), "the new query's rows");
        // Trusted by the paste's own frame: the first Down moves the bar.
        assert_eq!(
            list.keys(DOWN),
            at_window(1, 10, 2),
            "the list the paste left waited for another frame"
        );

        // A paste with nothing behind it, the same way; then an Enter of its
        // own takes the row the list marks.
        let rows = list.keys(b"\x1b[200~1\x1b[201~");
        assert_eq!(composer(&list.shell), format!("{COMPOSER_MARKER} @f-01"));
        assert_eq!(rows.first(), Some(&("items/f-01.txt".to_string(), true)));
        list.keys(b"\r");
        assert!(
            composer(&list.shell).contains("items/f-01.txt"),
            "{}",
            composer(&list.shell)
        );
        assert!(
            list.shell._work.try_recv().is_err(),
            "the Enter on a marked row sent the line"
        );
    }
