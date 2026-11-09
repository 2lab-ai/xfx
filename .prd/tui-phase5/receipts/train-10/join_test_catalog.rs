
    #[test]
    fn a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id(
    ) {
        // Train 10, join check 1 (MODEL-FUZZY x CATALOG-BOUNDS). How a read
        // fails is CATALOG-BOUNDS': an answer past fx's 16 MiB is one asking
        // again would only repeat (`retry: None`,
        // `provider::model::tests::a_gateway_catalog_one_byte_past_16_mib_fails_and_is_not_cut_to_fit`),
        // one still coming at the deadline is one xfx asks again about
        // (`Some(CatalogRetry::Unreachable)`,
        // `a_gateway_catalog_past_its_deadline_is_a_failure_asked_again_about`).
        // The runtime tells either as `CatalogUnread` when it held no rows and
        // as `CatalogRefreshFailed` behind rows it keeps, whatever the retry
        // (`Applied::of`, `provider/model.rs`). What the menu lists after it
        // is MODEL-FUZZY's: fx's three tiers over the whole id, each in
        // catalog order, and the mark kept on its model by id across a reload
        // (fx's own case, `model_cache_runtime.zig:2086-2123@481104fd`), here
        // with a failure before the first rows and one between two reads.
        let ids = |ids: &[&str]| -> Vec<CatalogEntry> {
            ids.iter()
                .map(|id| CatalogEntry {
                    id: id.to_string(),
                    aliases: Vec::new(),
                    name: None,
                    efforts: Vec::new(),
                    max_context: None,
                    facts: Default::default(),
                })
                .collect()
        };
        let first = [
            "openai/gpt-4.5-preview",
            "openai/gpt-5",
            "anthropic/claude-opus-4.8",
            "acme/gpt5-lite",
        ];
        let mut second = vec!["acme/gpt5-pro"];
        second.extend(first);
        let loaded = |entries: Vec<CatalogEntry>| UiEvent::CatalogLoaded {
            due: Instant::now() + REFRESH_INTERVAL,
            provider: ProviderId::Gateway,
            entries,
        };
        // The ids the menu lists, in its order.
        let listed = |shell: &Shell| -> Vec<String> {
            menu(shell)
                .iter()
                .filter_map(|row| {
                    visible_text(row)
                        .split_whitespace()
                        .find(|word| word.contains('/'))
                        .map(str::to_string)
                })
                .collect()
        };
        let marked = |shell: &Shell| {
            shell
                .model_menu
                .as_ref()
                .and_then(ModelMenu::selected)
                .map(str::to_string)
        };
        for (what, retry, says) in [
            ("past 16 MiB", None, "Unable to load models."),
            (
                "stalled to its deadline",
                Some(CatalogRetry::Unreachable),
                "Could not reach AI Gateway; xfx retries automatically.",
            ),
        ] {
            // The menu's first read fails; the next one loads.
            let mut shell = shell(40, 120);
            shell.route_bytes(b"/model\r");
            shell.apply(UiEvent::CatalogUnread {
                provider: ProviderId::Gateway,
                retry,
                retry_at: failed_long_ago(),
            });
            let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
            assert_eq!(rows, ["Models 0  [All]", "", says], "{what}");
            shell.route_bytes(b"gpt5");
            shell.apply(loaded(ids(&first)));
            assert_eq!(
                listed(&shell),
                ["acme/gpt5-lite", "openai/gpt-5", "openai/gpt-4.5-preview"],
                "{what}: the rows after the failure, substring then words then fuzzy"
            );
            assert_eq!(marked(&shell).as_deref(), Some("acme/gpt5-lite"), "{what}");
            shell.route_bytes(b"\x1b[B");
            assert_eq!(marked(&shell).as_deref(), Some("openai/gpt-5"), "{what}");

            // A refresh behind those rows fails the same way: rows and mark stand.
            shell.apply(UiEvent::CatalogRefreshFailed {
                provider: ProviderId::Gateway,
                retry,
                due: Instant::now() + crate::provider::model::failure_backoff(1),
            });
            assert_eq!(
                listed(&shell),
                ["acme/gpt5-lite", "openai/gpt-5", "openai/gpt-4.5-preview"],
                "{what}: the rows behind a failed refresh"
            );
            assert_eq!(
                marked(&shell).as_deref(),
                Some("openai/gpt-5"),
                "{what}: the mark behind a failed refresh"
            );

            // The read after it brings a better match: it ranks first, and the
            // mark stays on its model, now third.
            shell.apply(loaded(ids(&second)));
            assert_eq!(
                listed(&shell),
                [
                    "acme/gpt5-pro",
                    "acme/gpt5-lite",
                    "openai/gpt-5",
                    "openai/gpt-4.5-preview"
                ],
                "{what}: the rows of the read after the failed one"
            );
            assert_eq!(
                marked(&shell).as_deref(),
                Some("openai/gpt-5"),
                "{what}: the mark after the reload"
            );
        }
    }
