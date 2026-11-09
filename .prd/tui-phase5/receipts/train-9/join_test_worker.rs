    /// A scripted provider whose requests are counted where the test can read
    /// them after the runtime that owns it is gone.
    struct RequestsCounted {
        script: ScriptedProvider,
        asked: std::rc::Rc<std::cell::Cell<usize>>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for RequestsCounted {
        async fn stream(
            &self,
            request: &crate::gateway::protocol::CompletionRequest,
            deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            self.asked.set(self.asked.get() + 1);
            self.script.stream(request, deltas).await
        }
    }

    /// Twenty questions of two choices each, `Pick N?` with `Keep N` and
    /// `Change N`: the batch `p5-input-question20` asks.
    fn twenty_questions() -> serde_json::Value {
        serde_json::json!({ "questions": (1..=20).map(|index| serde_json::json!({
            "question": format!("Pick {index}?"),
            "options": [
                { "label": format!("Keep {index}") },
                { "label": format!("Change {index}") },
            ],
        })).collect::<Vec<_>>() })
    }

    #[test]
    fn a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop() {
        // Train 9, join check 2 (STOP-IN-CATALOG-WAIT x QUESTIONS-20). A
        // prompt with an image, on a model whose window is known, waits at its
        // start for the first catalog load, which lands 1.5 s in with a row
        // that reads images (`CatalogWait::PromptImageFirstLoad`); once asked,
        // the model asks twenty questions in one call. The band's questioner
        // is on the band's own channels, as `spawn` wires it, and the band
        // answers a batch it is shown.
        //
        // Without the stop the batch reaches the band whole -- twenty entries,
        // which QUESTIONS-20's cap lets through (`max_questions = 20`,
        // `question_answer.zig:9@481104fd`) -- is answered, and the turn
        // finishes after a second request. With a Ctrl-C 300 ms into the wait
        // the turn ends at once as the stop's own end (`resolveForRequest`,
        // `model_cache_runtime.zig:855-866`; `orchestrator.zig:5893-5919`,
        // `:5923-5948`): the model is never asked, no question reaches the
        // band, and the log keeps the prompt, its image and the stop, as fx's
        // interrupted turn keeps them (`interruption.zig:198-223`) -- the band
        // writes that end as fx's `▪ Cancelled`.
        let image = || {
            crate::image::Image::new(
                1,
                std::path::PathBuf::from("shot.png"),
                crate::image::MediaType::Png,
                std::sync::Arc::from(&b"png"[..]),
            )
        };
        let mut got = Vec::new();
        for stop in [Some(Duration::from_millis(300)), None] {
            let home = tempfile::tempdir().expect("a home");
            let workspace = tempfile::tempdir().expect("a workspace");
            let model = "anthropic/claude-sonnet-5";
            let env = Environment::new(
                Some(home.path().to_path_buf()),
                BTreeMap::from([("XFX_MODEL".to_string(), model.to_string())]),
            );
            let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
            let store = open_store(&config).expect("a store");
            let (events, mut seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
            let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
            let control = ControlChannel::new(control_rx);
            let cancel = Cancellation::new(CancelToken::new());
            let questioner = Arc::new(TuiQuestioner::new(
                events.clone(),
                Arc::clone(&control),
                cancel.clone(),
            ));
            let conversation = open_conversation(
                &store,
                &config,
                &config.model.clone(),
                PermissionSession::new(config.permission_mode),
                Some(Arc::clone(&questioner) as Arc<dyn crate::tools::QuestionRequester>),
                &CancelToken::new(),
            )
            .expect("open a conversation");
            let mut state = Runtime::with_environment(config, env, store, a_prompter(), questioner);
            state.conversation = Some(conversation);
            let asked = std::rc::Rc::new(std::cell::Cell::new(0));
            state.provider = Some(Bundle {
                id: ProviderId::Gateway,
                stream: Box::new(RequestsCounted {
                    script: ScriptedProvider::new(vec![
                        calls(vec![call("q20", "ask_user_question", twenty_questions())]),
                        answers("answered"),
                    ]),
                    asked: std::rc::Rc::clone(&asked),
                }),
            });
            let (open, gate) = tokio::sync::oneshot::channel::<()>();
            let mut row = published(model);
            row.facts.image_input = true;
            state.selector.set_fetcher_for_test(Box::new(Gated {
                entries: vec![row],
                gate: Mutex::new(Some(gate)),
            }));
            let (told, _told) = mpsc::channel(bridge::UI_EVENTS);
            on_a_runtime(async { begin_catalog_read(&mut state, &told) });
            assert!(state.selector.is_loading(), "no first load on its way");
            assert!(state.context_window().is_some(), "the window waits too");
            let sessions = state.store.sessions_dir().to_path_buf();
            let id = state
                .conversation
                .as_ref()
                .expect("a conversation")
                .recorder
                .id()
                .clone();
            let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(1);
            work_tx
                .try_send(TurnWork::Attached {
                    prompt: "first".to_string(),
                    images: vec![image()],
                    skills: Vec::new(),
                })
                .expect("room");
            drop(work_tx);
            let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
            let outstanding = AtomicUsize::new(1);
            let started = Instant::now();
            let opener = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1_500));
                let _ = open.send(());
            });
            // The band, on a thread of its own: the questioner parks the
            // runtime's thread while its panel is up, as it does in production.
            let band_control = control_tx.clone();
            let band = std::thread::spawn(move || {
                let mut log = Vec::new();
                let mut stopped = false;
                let deadline = Instant::now() + TEST_DRAIN;
                while Instant::now() < deadline {
                    if let Some(after) = stop {
                        if !stopped && started.elapsed() >= after {
                            let _ = band_control.send(TurnControl::Cancel { through: 1 });
                            stopped = true;
                        }
                    }
                    match seen.try_recv() {
                        Ok(event) => {
                            if let UiEvent::Question(request) = &event {
                                let answers = (1..=request.entries.len())
                                    .map(|index| format!("Keep {index}"))
                                    .collect();
                                let _ = band_control.send(TurnControl::QuestionAnswer {
                                    id: request.id,
                                    answers,
                                });
                            }
                            let ended = matches!(event, UiEvent::TurnEnded { .. });
                            log.push((started.elapsed(), event));
                            if ended {
                                break;
                            }
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(2)),
                    }
                }
                log
            });
            on_a_runtime(turn_loop(
                state,
                &events,
                &mut work_rx,
                &mut changes,
                control,
                cancel,
                &outstanding,
            ));
            let log = band.join().expect("the band");
            opener.join().expect("the load's opener");
            drop(control_tx);
            let shown: Vec<usize> = log
                .iter()
                .filter_map(|(_, event)| match event {
                    UiEvent::Question(request) => Some(request.entries.len()),
                    _ => None,
                })
                .collect();
            let ends: Vec<(Duration, Option<String>)> = log
                .iter()
                .filter_map(|(at, event)| match event {
                    UiEvent::TurnEnded { failure } => Some((*at, failure.clone())),
                    _ => None,
                })
                .collect();
            let on_time = ends.first().is_some_and(|(at, _)| match stop {
                // After the stop, and long before the load lands.
                Some(_) => *at >= Duration::from_millis(300) && *at < Duration::from_millis(1_000),
                None => *at >= Duration::from_millis(1_200),
            });
            got.push((
                stop.is_some(),
                shown,
                ends.into_iter()
                    .map(|(_, failure)| failure)
                    .collect::<Vec<_>>(),
                on_time,
                asked.get(),
                turns_logged(&sessions, &id),
            ));
        }
        let kept = |end: &str| {
            vec![
                "asked first".to_string(),
                "1 image(s)".to_string(),
                end.to_string(),
            ]
        };
        assert_eq!(
            got,
            [
                (
                    true,
                    Vec::new(),
                    vec![Some(bridge::TURN_CANCELLED.to_string())],
                    true,
                    0,
                    kept("interrupted cancelled"),
                ),
                (false, vec![20], vec![None], true, 2, kept("final stop")),
            ],
            "each: whether stopped, the batches the band was shown (their sizes), the turn's \
             end, whether it came when it should (with the stop: 300 ms to 1 s; without it: past \
             1.2 s), the model's requests, what the log says of the turn"
        );
    }
