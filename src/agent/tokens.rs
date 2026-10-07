//! What a turn's summary row counts: the tokens the turn moved.
//!
//! Upstream's rule (`vercel-labs/fx@c1db9191
//! src/core/agent/runtime/telemetry.zig:83-180`), in the two halves a turn has:
//!
//! - **In** is the estimate of the prompt that was submitted, once, when the
//!   turn begins (`TurnSummaryAccumulator.init`, `:87-96`). It does not move
//!   while the turn runs, however many requests the turn makes.
//! - **Out** is a sum over every model request the turn made: what the provider
//!   reported for each, or -- for one it reported nothing for, or one that
//!   failed, or one that was stopped -- the estimate of what that request
//!   streamed (`reconcileTokenRequest`, `:136-147`;
//!   `finishTokenRequestWithoutUsage`, `:149-155`; `settleOutputTokens`,
//!   `:165-168`). A request still streaming adds its estimate on top of the
//!   settled sum (`tokenProgress`, `:157-163`).
//!
//! A request is one transport attempt, not one step: upstream opens a token
//! request per attempt (`orchestrator.zig:7493`) and settles it whichever way
//! the attempt ends (`:7577`, `:8176`), so a retried step counts what its
//! failed attempt had already streamed.
//!
//! **What a request streamed is three estimates, kept apart**: the answer
//! text, the reasoning, and the tool-call arguments (`ActiveTokenRequest`,
//! `:25-35`). Not one estimate over the three run together -- a span that
//! ends one stream and a span that begins the next are two spans, each rounded
//! up on its own.

/// Upstream's streaming token estimate (`token_estimate.zig:5-31`): a
/// whitespace-separated span of `n` bytes is `ceil(n / 4)` tokens, and the
/// count is the same however the text was cut into chunks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Estimator {
    settled: u64,
    span: u64,
}

impl Estimator {
    pub fn consume(&mut self, text: &str) {
        for byte in text.bytes() {
            if separates(byte) {
                self.settled = self.settled.saturating_add(span_tokens(self.span));
                self.span = 0;
            } else {
                self.span = self.span.saturating_add(1);
            }
        }
    }

    pub fn estimate(&self) -> u64 {
        self.settled.saturating_add(span_tokens(self.span))
    }
}

/// Zig's `std.ascii.isWhitespace`: a space and `\t` through `\r`. Not
/// [`u8::is_ascii_whitespace`], which leaves out the vertical tab and so would
/// count `a\x0bb` as one span where upstream counts two.
fn separates(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t'..=b'\r')
}

fn span_tokens(span: u64) -> u64 {
    span / 4 + u64::from(!span.is_multiple_of(4))
}

/// What one model request has streamed so far, as its three estimates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Streamed {
    content: Estimator,
    reasoning: Estimator,
    tool_input: Estimator,
}

impl Streamed {
    /// Answer text, as it arrives.
    pub fn content(&mut self, text: &str) {
        self.content.consume(text);
    }

    /// Reasoning, which the user is never shown and the model still wrote.
    pub fn reasoning(&mut self, text: &str) {
        self.reasoning.consume(text);
    }

    /// A tool call's arguments as the model writes them. Counted the same way
    /// the answer is: without them a request that spends its output on one
    /// large call reads as a frozen count until the provider settles it
    /// (`telemetry.zig:122-126`).
    pub fn tool_input(&mut self, text: &str) {
        self.tool_input.consume(text);
    }

    /// The whole estimate: what the request is said to have written when
    /// nobody reports what it did.
    pub fn estimate(&self) -> u64 {
        self.content.estimate().saturating_add(self.unshown())
    }

    /// The part of it that is not answer text -- what a reader of the stream
    /// has not seen, and so cannot count for itself.
    pub fn unshown(&self) -> u64 {
        self.reasoning
            .estimate()
            .saturating_add(self.tool_input.estimate())
    }
}

/// What one request adds to the turn's out: the provider's count when it gave
/// one -- a reported nought included -- and otherwise the estimate of what the
/// request streamed (`usage.output_tokens orelse active.outputEstimate()`,
/// `telemetry.zig:140`).
pub fn settled_output(reported: Option<u64>, streamed: &Streamed) -> u64 {
    reported.unwrap_or_else(|| streamed.estimate())
}

/// What a turn tells whoever is showing it about its out, as it moves
/// ([`crate::output::EventSink::output_tokens`]).
///
/// **Two facts rather than one running total**, because the answer text is
/// already on its way to the reader as deltas, and a reader counts it as it
/// arrives -- upstream's live row moves with every chunk
/// (`assistant_stream.zig:235-246`). A total per chunk would be a second event
/// for every delta. What the reader cannot count for itself is the rest: where
/// a request ended, and what a request is writing that the reader is not
/// shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// A request has ended, and this is the turn's out over every request that
    /// has: the reader's own count of answer text starts again from nothing.
    Settled(u64),
    /// The open request's estimate of its reasoning and tool-call arguments so
    /// far ([`Streamed::unshown`]).
    Unshown(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(text: &str) -> u64 {
        let mut estimator = Estimator::default();
        estimator.consume(text);
        estimator.estimate()
    }

    #[test]
    fn a_span_is_a_quarter_of_its_bytes_rounded_up_however_it_was_cut() {
        assert_eq!(estimate(""), 0);
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("abcde"), 2);
        assert_eq!(estimate("hello"), 2);
        assert_eq!(estimate("run the command"), 4);
        let text = "split 你好 inside words";
        for cut in 0..=text.len() {
            if !text.is_char_boundary(cut) {
                continue;
            }
            let mut pieces = Estimator::default();
            pieces.consume(&text[..cut]);
            pieces.consume(&text[cut..]);
            assert_eq!(pieces.estimate(), estimate(text), "cut at {cut}");
        }
    }

    #[test]
    fn every_separator_upstream_splits_on_ends_a_span_the_vertical_tab_included() {
        // Two one-byte spans each time: one token apiece. Rust's own
        // `is_ascii_whitespace` would make `a\x0bb` one span, one token.
        for separator in [' ', '\t', '\n', '\x0b', '\x0c', '\r'] {
            assert_eq!(estimate(&format!("a{separator}b")), 2, "{separator:?}");
        }
        assert_eq!(estimate("a\x1fb"), 1, "a unit separator is not whitespace");
    }

    #[test]
    fn a_request_streamed_three_estimates_and_not_one_over_their_concatenation() {
        let mut streamed = Streamed::default();
        streamed.content("ab");
        streamed.reasoning("cd");
        streamed.tool_input("{\"x\":1}");
        // `abcd{"x":1}` as one span would be 3; apart they are 1 + 1 + 2.
        assert_eq!(streamed.estimate(), 4);
        assert_eq!(streamed.unshown(), 3);
    }

    #[test]
    fn a_reported_count_wins_over_the_estimate_even_when_it_is_nought() {
        let mut streamed = Streamed::default();
        streamed.content("twelve bytes");
        assert_eq!(settled_output(Some(40), &streamed), 40);
        assert_eq!(settled_output(Some(0), &streamed), 0);
        assert_eq!(settled_output(None, &streamed), 4);
    }
}
