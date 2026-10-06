//! A saved session, as the value the shell replays onto the screen.
//!
//! Upstream replays a resumed session's turns into the transcript through the
//! adapters a live turn goes through (`hydrateResumedSession` and
//! `replayHistoryToSinkIncremental`, `vercel-labs/fx@c1db9191
//! src/core/app/app_session_runtime.zig:2368-2470`, `:4753-4810`): the resume
//! notice first, then for every turn the user's card, what the assistant did,
//! and the turn's summary. This module is the half of that which reads the log:
//! a [`Hydration`] is built from a [`DurableState`] on the side that holds the
//! session -- the runtime thread, or the launch before the terminal is taken --
//! and handed across as plain text, so the thread that holds the terminal never
//! reads a file for it. What the shell writes from it is `super::shell`'s
//! `Shell::hydrate`, through the same card, presenter, tool group and summary a
//! live turn's events reach.
//!
//! The steps keep the log's order, which is upstream's: each tool step's text,
//! then its calls and their results (`writeExecutionHistoryToSink`, `:4875-4920`,
//! run at `:4769` and `:4789`), then the turn's closing answer (`:4773-4775`,
//! `:4793-4795`) -- the text of the step that asked for no tool, which is the
//! log's last step. A step none of whose calls came back is not replayed, text
//! and all, and a call that never came back has no row: upstream's history
//! keeps only the calls with a result, and only the steps with one
//! (`buildNormalExecutionMemory`, `execution_memory.zig:449-452`,
//! `dupeCompletedPersistedToolCalls`). A call the turn's stop cancelled is one
//! of those -- upstream records no result for it, xfx's log does, marked.

use crate::agent::TurnError;
use crate::session::{DurableState, HistoryTurn, LineCounts, ToolEnding, TurnStep};
use crate::tools::{LineChanges, Outcome, Registry, Shown};

/// A resumed session, ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hydration {
    /// What the session is called ([`DurableState::title`]): the resume
    /// notice's body and the window's title. `None` for one with no title.
    pub(crate) title: Option<String>,
    pub(crate) turns: Vec<HydratedTurn>,
    /// The model the runtime continues the session in, for the hint row. `None`
    /// where the caller already shows the right one -- the launch, whose band
    /// is built from the configuration the session's model was restored into.
    pub(crate) model: Option<String>,
    /// The number the next attached image takes: one past the highest the
    /// session's prompts already name, so `[Image #N]` never means two images
    /// in one conversation (upstream rebases a resumed session's ids the same
    /// way, `vision-route-fake-gateway.test.ts:1871`).
    pub(crate) next_image: u32,
}

/// One turn of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HydratedTurn {
    /// The prompt as its card shows it: the log's text, with each image's
    /// `[Image #N]` spelled as the badge the composer showed, `[Image N]`.
    pub(crate) user: String,
    /// The prompt exactly as the log holds it -- what the model was sent, and
    /// what the turn's summary counts, as a live turn's does.
    pub(crate) sent: String,
    pub(crate) steps: Vec<HydratedStep>,
    pub(crate) ending: Ending,
    /// When it started and when it concluded, as the log has them
    /// ([`crate::session::TurnTiming`]): the times its entries in the full
    /// transcript carry.
    pub(crate) started_at_ms: i64,
    pub(crate) concluded_at_ms: Option<i64>,
    /// What the full transcript shows of each call that the screen does not:
    /// its arguments and its whole result
    /// ([`super::full_transcript::details_of`]).
    pub(crate) details: Vec<(String, super::full_transcript::Detail)>,
}

/// One thing a turn did, in the order it did it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HydratedStep {
    /// Answer text, raw markdown, for the presenter.
    Text(String),
    /// A call, named as a live turn named it before it ran.
    ToolStart { id: String, shown: Shown },
    /// What came back, as the live row said it: the outcome and the lines a
    /// change made, as the log recorded them ([`ToolEnding`], [`LineCounts`]).
    /// A log written before they were recorded settles every call as done or
    /// failed.
    ///
    /// No failure's reason: upstream's replayed row is `Failed missing.txt`
    /// where the live one said why (`subagentFailureLabel`, the label only,
    /// `app_session_runtime.zig:5036-5042`; capture
    /// `fx-resume-tools-40x120-resumed`).
    ToolResult {
        id: String,
        ok: bool,
        outcome: Outcome,
        changes: Option<LineChanges>,
    },
    /// What the user said at an approval, as the live amendment row says it.
    Feedback(String),
}

/// How a turn ended, as far as its summary row is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ending {
    /// It concluded: the summary row, from how long it took and what the
    /// provider said it wrote (`None` when it said nothing).
    Concluded { seconds: u64, output: Option<u64> },
    /// The user stopped it: upstream's turn cancellation line
    /// (`appendTurnCancellation`, `app_session_runtime.zig:3994-4005`, written
    /// at `:4796-4800`).
    Cancelled,
    /// The user stopped the automatic compaction it began with: upstream
    /// writes nothing after the card, its cancellation line being only for a
    /// stop whose origin is the turn (`:4796-4800`, `cancellation_origin`;
    /// e2e `tui-compaction-activity.test.ts:481-488`).
    CompactionCancelled,
    /// It failed: upstream's `✗ system: failed` (`interruptedTurnNotice`,
    /// `session.zig:45-50`, written at `:4801-4803`), whatever the live turn's
    /// sentence was.
    Failed,
    /// Its conclusion never reached the log: a process that died mid-turn.
    Unconcluded,
}

impl Hydration {
    /// What `state` replays as.
    pub(crate) fn of(state: &DurableState, model: Option<String>) -> Self {
        Self {
            title: state.title(),
            turns: state.turns.iter().map(turn).collect(),
            model,
            next_image: state
                .turns
                .iter()
                .flat_map(|turn| turn.images.iter())
                .map(|image| image.recorded.id.saturating_add(1))
                .max()
                .unwrap_or(1),
        }
    }
}

fn turn(turn: &HistoryTurn) -> HydratedTurn {
    let registry = Registry::builtin();
    // The calls that came back, and with what: a result the turn's stop
    // cancelled is not one of them.
    let answered: Vec<&str> = turn
        .steps
        .iter()
        .filter_map(|step| match step {
            TurnStep::ToolResult {
                call_id, ending, ..
            } if *ending != Some(ToolEnding::Cancelled) => Some(call_id.as_str()),
            _ => None,
        })
        .collect();
    let mut steps = Vec::new();
    for step in &turn.steps {
        match step {
            TurnStep::Assistant {
                text, tool_calls, ..
            } => {
                let came_back: Vec<_> = tool_calls
                    .iter()
                    .filter(|call| answered.contains(&call.id.as_str()))
                    .collect();
                if !tool_calls.is_empty() && came_back.is_empty() {
                    continue;
                }
                if !text.is_empty() {
                    steps.push(HydratedStep::Text(text.clone()));
                }
                // A call to a tool this build does not offer never started on
                // a live screen either: the turn ended on it before any row
                // (`agent::machine`'s `ToolCallUnsupported`).
                steps.extend(came_back.into_iter().filter_map(|call| {
                    registry
                        .spec(&call.name)
                        .map(|spec| HydratedStep::ToolStart {
                            id: call.id.clone(),
                            shown: spec.shown(&call.input),
                        })
                }));
            }
            TurnStep::ToolResult {
                call_id,
                ok,
                ending,
                lines,
                ..
            } => {
                if *ending == Some(ToolEnding::Cancelled) {
                    continue;
                }
                steps.push(HydratedStep::ToolResult {
                    id: call_id.clone(),
                    ok: *ok,
                    outcome: outcome(*ending),
                    changes: lines.map(changes),
                });
            }
            TurnStep::ToolFeedback { text, .. } => {
                steps.push(HydratedStep::Feedback(text.clone()));
            }
        }
    }
    let ending = match &turn.outcome {
        Some(crate::session::TurnConclusion::Final { .. }) => {
            let timing = turn.timing;
            let elapsed = timing
                .concluded_at_ms
                .map_or(0, |end| end.saturating_sub(timing.started_at_ms).max(0));
            Ending::Concluded {
                seconds: u64::try_from(elapsed / 1_000).unwrap_or(0),
                output: timing.output_tokens,
            }
        }
        // The log keeps the sentence the live turn ended on, and a stop's is
        // always this one ([`TurnError::Cancelled`]) -- or, for a stop the
        // automatic compaction took, the compaction's own.
        Some(crate::session::TurnConclusion::Interrupted { reason })
            if *reason == TurnError::Cancelled.to_string() =>
        {
            Ending::Cancelled
        }
        Some(crate::session::TurnConclusion::Interrupted { reason })
            if *reason == super::compaction::Feedback::Cancelled.label() =>
        {
            Ending::CompactionCancelled
        }
        Some(crate::session::TurnConclusion::Interrupted { .. }) => Ending::Failed,
        None => Ending::Unconcluded,
    };
    // The card says what the composer said: each image's badge, `[Image N]`,
    // where the prompt the model was sent says `[Image #N]`.
    let mut user = turn.user.clone();
    for image in &turn.images {
        user = user.replace(
            &crate::image::token(image.recorded.id),
            &crate::image::badge(image.recorded.id),
        );
    }
    HydratedTurn {
        user,
        sent: turn.user.clone(),
        steps,
        ending,
        started_at_ms: turn.timing.started_at_ms,
        concluded_at_ms: turn.timing.concluded_at_ms,
        details: super::full_transcript::details_of(&turn.steps),
    }
}

/// The row's outcome, from what the log recorded of it.
fn outcome(ending: Option<ToolEnding>) -> Outcome {
    match ending {
        None | Some(ToolEnding::Other) => Outcome::Finished,
        Some(ToolEnding::Unchanged) => Outcome::Unchanged,
        Some(ToolEnding::Denied) => Outcome::Denied,
        Some(ToolEnding::Exited { code }) => Outcome::Exited(code),
        Some(ToolEnding::Signalled { signal }) => Outcome::Signalled(signal),
        Some(ToolEnding::TimedOut) => Outcome::TimedOut,
        Some(ToolEnding::Cancelled) => Outcome::Cancelled,
    }
}

fn changes(lines: LineCounts) -> LineChanges {
    LineChanges {
        added: usize::try_from(lines.added).unwrap_or(usize::MAX),
        removed: usize::try_from(lines.removed).unwrap_or(usize::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::session::{RecordedToolCall, TurnConclusion, TurnTiming};
    use serde_json::json;

    fn concluded(user: &str, steps: Vec<TurnStep>, timing: TurnTiming) -> HistoryTurn {
        HistoryTurn {
            user: user.to_string(),
            images: Vec::new(),
            steps,
            outcome: Some(TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            }),
            timing,
            seqs: crate::session::TurnSeqs::default(),
        }
    }

    #[test]
    fn a_replayed_card_shows_the_badges_and_the_summary_counts_what_was_sent() {
        // `fx-image-40x120-resumed`: the replayed card is `┃ [Image 3][Image 4]
        // what is in this image?` and its summary `↑12`, the count of the
        // prompt the model was sent.
        let image = |id: u32| crate::session::TurnImage {
            recorded: crate::session::RecordedImage {
                id,
                path: "/w/shot.png".to_string(),
                media_type: "image/png".to_string(),
                sha256: "0".repeat(64),
                bytes: 73,
            },
            snapshot: crate::session::Snapshot::Unread,
        };
        let mut turn = concluded(
            "[Image #3][Image #4] what is in this image? [Image #9]",
            Vec::new(),
            TurnTiming::default(),
        );
        turn.images = vec![image(3), image(4)];
        let state = DurableState {
            id: "s".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "m".to_string(),
            permission_mode: crate::config::PermissionMode::Auto,
            turns: vec![turn],
            grants: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            context_sources: Vec::new(),
            last_event_seq: 0,
            renamed: None,
            usage: crate::session::RecordedUsage::default(),
            checkpoint: None,
        };
        let replay = Hydration::of(&state, None);
        // `#9` names no image of the turn: those are the user's own words.
        assert_eq!(
            replay.turns[0].user,
            "[Image 3][Image 4] what is in this image? [Image #9]"
        );
        assert_eq!(
            replay.turns[0].sent,
            "[Image #3][Image #4] what is in this image? [Image #9]"
        );
        assert_eq!(replay.next_image, 5);
    }

    #[test]
    fn a_turn_replays_its_steps_in_the_order_the_log_holds_them() {
        let turn = concluded(
            "read it",
            vec![
                TurnStep::Assistant {
                    text: "Looking.".to_string(),
                    tool_calls: vec![
                        RecordedToolCall {
                            id: "c1".to_string(),
                            name: "read_file".to_string(),
                            input: json!({ "path": "a.txt" }),
                        },
                        RecordedToolCall {
                            id: "c2".to_string(),
                            name: "no_such_tool".to_string(),
                            input: json!({}),
                        },
                    ],
                    raw_content: Vec::new(),
                    responses_state: Vec::new(),
                    wire: None,
                },
                TurnStep::ToolResult {
                    ending: None,
                    lines: None,
                    change: None,
                    call_id: "c1".to_string(),
                    tool: "read_file".to_string(),
                    ok: false,
                    output: "no such path: `a.txt`\nmore".to_string(),
                },
                TurnStep::ToolFeedback {
                    call_id: "c1".to_string(),
                    text: "use b".to_string(),
                },
                TurnStep::Assistant {
                    text: "Done.".to_string(),
                    tool_calls: Vec::new(),
                    raw_content: Vec::new(),
                    responses_state: Vec::new(),
                    wire: None,
                },
            ],
            TurnTiming {
                started_at_ms: 10_000,
                concluded_at_ms: Some(12_999),
                output_tokens: Some(40),
            },
        );
        let replayed = super::turn(&turn);
        assert_eq!(replayed.user, "read it");
        assert_eq!(replayed.steps.len(), 5, "{:?}", replayed.steps);
        assert_eq!(
            replayed.steps[0],
            HydratedStep::Text("Looking.".to_string())
        );
        match &replayed.steps[1] {
            HydratedStep::ToolStart { id, shown } => {
                assert_eq!(id, "c1");
                assert_eq!(shown.target.as_deref(), Some("a.txt"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            replayed.steps[2],
            HydratedStep::ToolResult {
                id: "c1".to_string(),
                ok: false,
                outcome: Outcome::Finished,
                changes: None,
            }
        );
        assert_eq!(
            replayed.steps[3],
            HydratedStep::Feedback("use b".to_string())
        );
        assert_eq!(replayed.steps[4], HydratedStep::Text("Done.".to_string()));
        assert_eq!(
            replayed.ending,
            Ending::Concluded {
                seconds: 2,
                output: Some(40)
            }
        );
        // And what the full transcript shows of the calls, with the times its
        // entries carry: every call the log has, the one no row is drawn for
        // included -- its details wait for a row that never comes and go
        // with the turn (`super::super::full_transcript`'s `turn_over`).
        assert_eq!(
            (replayed.started_at_ms, replayed.concluded_at_ms),
            (10_000, Some(12_999))
        );
        let ids: Vec<&str> = replayed.details.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["c1", "c2", "c1"]);
        assert_eq!(
            replayed.details[2].1,
            super::super::full_transcript::Detail::Result(
                "no such path: `a.txt`\nmore".to_string()
            )
        );
    }

    fn call(id: &str, name: &str, input: serde_json::Value) -> RecordedToolCall {
        RecordedToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input,
        }
    }

    fn asked(text: &str, calls: Vec<RecordedToolCall>) -> TurnStep {
        TurnStep::Assistant {
            text: text.to_string(),
            tool_calls: calls,
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        }
    }

    fn came_back(
        id: &str,
        ok: bool,
        ending: Option<ToolEnding>,
        lines: Option<LineCounts>,
    ) -> TurnStep {
        TurnStep::ToolResult {
            call_id: id.to_string(),
            tool: "t".to_string(),
            ok,
            output: "out".to_string(),
            ending,
            lines,
            change: None,
        }
    }

    #[test]
    fn a_result_replays_with_the_outcome_and_the_lines_its_row_had() {
        let turn = concluded(
            "fix it",
            vec![
                asked(
                    "",
                    vec![
                        call("e", "edit_file", json!({ "path": "a.txt" })),
                        call("w", "write_file", json!({ "path": "c.txt" })),
                        call(
                            "x",
                            "terminal",
                            json!({ "action": "exec", "command": "exit 3" }),
                        ),
                        call("n", "edit_file", json!({ "path": "b.txt" })),
                    ],
                ),
                came_back(
                    "e",
                    true,
                    None,
                    Some(LineCounts {
                        added: 1,
                        removed: 1,
                    }),
                ),
                came_back("w", false, Some(ToolEnding::Denied), None),
                came_back("x", true, Some(ToolEnding::Exited { code: 3 }), None),
                came_back("n", true, Some(ToolEnding::Unchanged), None),
            ],
            TurnTiming::default(),
        );
        let results: Vec<(Outcome, Option<LineChanges>)> = super::turn(&turn)
            .steps
            .into_iter()
            .filter_map(|step| match step {
                HydratedStep::ToolResult {
                    outcome, changes, ..
                } => Some((outcome, changes)),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            [
                (
                    Outcome::Finished,
                    Some(LineChanges {
                        added: 1,
                        removed: 1
                    })
                ),
                (Outcome::Denied, None),
                (Outcome::Exited(3), None),
                (Outcome::Unchanged, None),
            ]
        );
    }

    /// Upstream's history keeps only the calls that came back, and only the
    /// steps with one (`execution_memory.zig:449-452`): a call the stop
    /// cancelled has no row, and a step all of whose calls were cancelled is
    /// not replayed, its text included.
    #[test]
    fn a_call_the_stop_cancelled_and_a_step_with_nothing_back_are_not_replayed() {
        let mut turn = concluded(
            "go",
            vec![
                asked(
                    "First.",
                    vec![
                        call("r", "read_file", json!({ "path": "a.txt" })),
                        call("s", "read_file", json!({ "path": "b.txt" })),
                    ],
                ),
                came_back("r", true, None, None),
                asked(
                    "Then.",
                    vec![call("w", "write_file", json!({ "path": "c.txt" }))],
                ),
                came_back("w", false, Some(ToolEnding::Cancelled), None),
            ],
            TurnTiming::default(),
        );
        turn.outcome = Some(TurnConclusion::Interrupted {
            reason: TurnError::Cancelled.to_string(),
        });
        let replayed = super::turn(&turn);
        let ids: Vec<String> = replayed
            .steps
            .iter()
            .map(|step| match step {
                HydratedStep::Text(text) => text.clone(),
                HydratedStep::ToolStart { id, .. } => format!("start {id}"),
                HydratedStep::ToolResult { id, .. } => format!("result {id}"),
                HydratedStep::Feedback(text) => text.clone(),
            })
            .collect();
        assert_eq!(ids, ["First.", "start r", "result r"]);
        assert_eq!(replayed.ending, Ending::Cancelled);
    }

    #[test]
    fn a_stop_is_a_cancellation_any_other_interruption_a_failure_and_an_unfinished_turn_nothing() {
        let mut stopped = concluded("q", Vec::new(), TurnTiming::default());
        // The sentence a stop is logged with, pinned: a log written with it
        // must keep replaying as a cancellation.
        assert_eq!(TurnError::Cancelled.to_string(), "the turn was cancelled");
        stopped.outcome = Some(TurnConclusion::Interrupted {
            reason: "the turn was cancelled".to_string(),
        });
        assert_eq!(super::turn(&stopped).ending, Ending::Cancelled);
        stopped.outcome = Some(TurnConclusion::Interrupted {
            reason: "the provider failed: overloaded".to_string(),
        });
        assert_eq!(super::turn(&stopped).ending, Ending::Failed);
        // A stop the automatic compaction took is logged with the
        // compaction's sentence, and is not the turn's cancellation.
        stopped.outcome = Some(TurnConclusion::Interrupted {
            reason: super::super::compaction::Feedback::Cancelled
                .label()
                .to_string(),
        });
        assert_eq!(super::turn(&stopped).ending, Ending::CompactionCancelled);
        stopped.outcome = None;
        assert_eq!(super::turn(&stopped).ending, Ending::Unconcluded);
    }
}
