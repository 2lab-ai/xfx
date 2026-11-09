
    #[test]
    fn a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs(
    ) {
        // Train 10, join check 2 (COMPACT-NOTICE-LATE x HISTORY-NOTICE-STOP),
        // on the real `turn_loop`. Three submissions: a prompt stopped in its
        // catalog wait before its request (HISTORY-NOTICE-STOP's `WindowRetry`
        // case), a `/compact` queued behind it, and a prompt after them. A
        // Ctrl-C quoting the first two, 300 ms in: the stopped turn hands no
        // history notice on, so the notice is the third submission's turn's,
        // said once (`one_turn` marks it said only once `send_ui` has queued
        // it); the compaction the stop dropped before it began is told the
        // band once, `Dropped`, inside the stop (`Queue::abandon` ->
        // `try_send_terminal`), before the next turn starts; and every place
        // is given back.
        let (_home, _workspace, mut state, first, held) = waiting_in(CatalogWait::WindowRetry);
        sealed_on_llmux(&mut state);
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(3);
        work_tx.try_send(first).expect("room");
        work_tx.try_send(TurnWork::Compact).expect("room");
        work_tx
            .try_send(TurnWork::Submit("third".to_string()))
            .expect("room");
        drop(work_tx);
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        let (events, mut seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let outstanding = AtomicUsize::new(3);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let _ = control_tx.send(TurnControl::Cancel { through: 2 });
            // Held until the loop has run out of work: a closed control
            // channel is no stop.
            std::thread::sleep(Duration::from_secs(8));
        });
        let log = on_a_runtime(async {
            let run = turn_loop(
                state,
                &events,
                &mut work_rx,
                &mut changes,
                ControlChannel::new(control_rx),
                Cancellation::new(CancelToken::new()),
                &outstanding,
            );
            let watch = async {
                let mut log = Vec::new();
                let mut ends = 0;
                while ends < 2 {
                    match tokio::time::timeout(TEST_DRAIN, seen.recv()).await {
                        Ok(Some(event)) => {
                            if matches!(event, UiEvent::TurnEnded { .. }) {
                                ends += 1;
                            }
                            log.push(event);
                        }
                        _ => break,
                    }
                }
                log
            };
            let (_, log) = tokio::join!(run, watch);
            log
        });
        drop(stopper);
        drop(held);
        let mut ends = Vec::new();
        let mut said: Vec<usize> = Vec::new();
        let mut compactions = Vec::new();
        for event in &log {
            match event {
                UiEvent::TurnStarted { .. } => said.push(0),
                UiEvent::TurnEnded { failure } => ends.push(failure.clone()),
                UiEvent::Notice(notice) if notice.text().contains(NOT_CARRIED) => {
                    *said.last_mut().expect("a notice inside a turn") += 1;
                }
                UiEvent::Compaction(update) => {
                    compactions.push((said.len(), format!("{update:?}")))
                }
                _ => {}
            }
        }
        assert_eq!(
            (ends, said, compactions),
            (
                vec![Some(bridge::TURN_CANCELLED.to_string()), None],
                vec![0, 1],
                vec![(1, "Dropped".to_string())],
            ),
            "the two turns' ends; the history notice said per turn; each compaction word, \
             with how many turns had started"
        );
        assert_eq!(outstanding.load(Ordering::Acquire), 0, "a place was kept");
    }
