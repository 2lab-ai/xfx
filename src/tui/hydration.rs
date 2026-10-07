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
//! The steps keep the log's order, which is the order a live turn showed them
//! in: text, the calls it asked for, their results, the next text. Upstream's
//! replay writes a turn's tool calls before the whole of its answer (`:4769`,
//! `:4773-4775`), because its history keeps the answer as one string; xfx's log
//! keeps each step, so a resumed screen reads as the live one did.

use crate::output::safe_one_line;
use crate::session::{DurableState, HistoryTurn, TurnStep};
use crate::tools::{Outcome, Registry, Shown, ToolResult};

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
    /// What came back.
    ///
    /// The log keeps what the model was told -- `ok` and the output -- and not
    /// how the call ended for the person watching (an exit status, a denial,
    /// the lines a change added), so a call settles here as done or as failed
    /// with its first line, and nothing more specific.
    ToolResult {
        id: String,
        ok: bool,
        detail: String,
        outcome: Outcome,
    },
    /// What the user said at an approval, as the live amendment row says it.
    Feedback(String),
}

/// How a turn ended, as far as its summary row is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ending {
    /// It concluded: the summary row, from how long it took and the out its
    /// live summary said, as the log kept it
    /// ([`crate::session::TurnTiming::output_tokens`]). `None` where the log
    /// kept no count at all, and the replay estimates the text it has instead.
    Concluded { seconds: u64, output: Option<u64> },
    /// It stopped, for this reason -- the sentence the live turn ended on.
    Interrupted(String),
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
    let mut steps = Vec::new();
    for step in &turn.steps {
        match step {
            TurnStep::Assistant {
                text, tool_calls, ..
            } => {
                if !text.is_empty() {
                    steps.push(HydratedStep::Text(text.clone()));
                }
                // A call to a tool this build does not offer never started on
                // a live screen either: the turn ended on it before any row
                // (`agent::machine`'s `ToolCallUnsupported`).
                steps.extend(tool_calls.iter().filter_map(|call| {
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
                output,
                ..
            } => steps.push(HydratedStep::ToolResult {
                id: call_id.clone(),
                ok: *ok,
                // A refusal's row names its first line, which is what the live
                // result's summary was (`ToolResult::failure`).
                detail: if *ok {
                    String::new()
                } else {
                    ToolResult::failure(output.as_str()).detail
                },
                outcome: Outcome::Finished,
            }),
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
        Some(crate::session::TurnConclusion::Interrupted { reason }) => {
            Ending::Interrupted(safe_one_line(reason, 1_024))
        }
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
                detail: "no such path: `a.txt`".to_string(),
                outcome: Outcome::Finished,
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

    #[test]
    fn a_stopped_turn_ends_on_its_reason_and_an_unfinished_one_on_nothing() {
        let mut stopped = concluded("q", Vec::new(), TurnTiming::default());
        stopped.outcome = Some(TurnConclusion::Interrupted {
            reason: "the turn was cancelled".to_string(),
        });
        assert_eq!(
            super::turn(&stopped).ending,
            Ending::Interrupted("the turn was cancelled".to_string())
        );
        stopped.outcome = None;
        assert_eq!(super::turn(&stopped).ending, Ending::Unconcluded);
    }
}
