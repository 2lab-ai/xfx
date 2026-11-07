//! Upstream's presenter tests (`assistant_presentation.zig:86-3935`), ported
//! with their inputs and expected bytes unchanged.
//!
//! Upstream reads OSC 8 link ids off one process-wide counter, so its tests
//! capture the counter before rendering; here each test reads its own
//! presenter's counter the same way. A completion upstream passes as a
//! `ctx` plus a function pointer is a closure here, and a capture that
//! upstream mutates through `ctx` is a `Cell` the closure updates.
//!
//! Four upstream tests are not ported because they exercise allocation
//! failure, which has no counterpart here: `setext lookahead frees
//! allocation failures` (:2176), `definition lists free allocation failures`
//! (:2213), `footnotes free allocation failures` (:2380), and `code block
//! payload clone frees language if code allocation fails` (:3223).

use std::time::Duration;

use super::payload::{CodeBlockCompletion, ThematicRuleCompletion};
use super::{MarkdownCompletions, MarkdownProcessor};

mod blocks;
mod code_tables;
mod events;
mod inline;
mod links;
mod setext_footnotes;

/// How many times longer the long line is than a short one in
/// [`assert_renders_in_linear_time`], and how many short lines its render is
/// timed against.
const LINEAR_GROWTH: usize = 16;

/// The most the long line's render may cost, as a multiple of the
/// [`LINEAR_GROWTH`] short lines' together. The two inputs carry the same
/// bytes, so a linear pass costs about as much for one as for the other (1)
/// and a quadratic one [`LINEAR_GROWTH`] (16) times as much; 4 sits halfway
/// between them on a log scale, so either reading can be off by a factor of
/// four before the check gets it wrong.
const LINEAR_RATIO_LIMIT: f64 = 4.0;

/// Each input is rendered this many times a round, alternating, and the
/// cheapest run of each is compared. Noise only ever adds time, so the
/// minimum is the closest reading of the work itself.
const LINEAR_RUNS: usize = 3;

/// A round whose ratio reaches the limit is measured again, up to this many
/// rounds in all, and the check fails only if every round does. A slow spell
/// that lands on every long run of one round does not repeat round after
/// round. A quadratic render reads about 16 in every round. To pass one, all
/// of that round's short runs would have to be slowed four times while one of
/// its long runs, a window 16 times as long, was not.
const LINEAR_ROUNDS: usize = 3;

/// A backstop against a render that never ends; it is not the linearity
/// check. The slowest line here costs about 50 ms of CPU in a debug build on
/// an M5 Max, and the `macos-15-intel` runner has been seen 17 times slower
/// than that machine, which still leaves this ceiling about 12 times clear.
const RENDER_CEILING: Duration = Duration::from_secs(10);

/// Asserts that rendering `line(n)` costs about as much as rendering
/// [`LINEAR_GROWTH`] copies of `line(n / LINEAR_GROWTH)` in one push, and
/// returns the bytes `line(n)` rendered to.
///
/// The claim is that the renderer is linear, not that the machine is fast,
/// and a fixed budget cannot tell those apart: on PR #57's `macos-15-intel`
/// runner (2026-11-06) a linear 400 KB line took 568 ms of CPU against a
/// 500 ms limit, where an M5 Max takes 34 ms. Two inputs rendered by one
/// thread moments apart share the machine's speed, so their ratio keeps only
/// how the cost grows.
///
/// The two inputs carry the same bytes, so for a linear render the two
/// timed windows are the same length. A machine whose speed changes from
/// moment to moment needs that: PR #62's `macos-15` arm64 runner
/// (2026-11-06) read two lines of equal cost as 424 µs and 866 µs within a
/// second. Against one line 4 times longer, a short line's window is a
/// quarter as long and so likelier to find a fast moment for its cheapest
/// run; timed that way, a linear line read 11.94 there against a limit of 8.
/// Windows of one length draw their cheapest runs from the same spread.
#[track_caller]
fn assert_renders_in_linear_time(label: &str, n: usize, line: impl Fn(usize) -> String) -> Vec<u8> {
    assert_eq!(n % LINEAR_GROWTH, 0, "{label}: x{n} splits evenly");
    let short_n = n / LINEAR_GROWTH;
    let short_lines = line(short_n).repeat(LINEAR_GROWTH);
    let long_line = line(n);
    let short_what = format!("{LINEAR_GROWTH} lines of x{short_n}");
    let long_what = format!("x{n}");
    let mut processor = MarkdownProcessor::new();
    let mut short_out = Vec::new();
    let mut long_out = Vec::new();
    let mut rounds = Vec::new();
    for round in 1..=LINEAR_ROUNDS {
        let mut short = Duration::MAX;
        let mut long = Duration::MAX;
        for _ in 0..LINEAR_RUNS {
            for (what, input, out, best) in [
                (&short_what, &short_lines, &mut short_out, &mut short),
                (&long_what, &long_line, &mut long_out, &mut long),
            ] {
                out.clear();
                let started = thread_cpu_time();
                processor.push(input, out);
                let elapsed = thread_cpu_time() - started;
                assert!(
                    elapsed < RENDER_CEILING,
                    "{label} {what} took {elapsed:?}, past the {RENDER_CEILING:?} ceiling"
                );
                // Every line is complete, so this push rendered each one
                // instead of holding it back for the next push to pay for.
                assert_eq!(
                    newlines(out),
                    newlines(input.as_bytes()),
                    "{label} {what} renders every line in its own push"
                );
                *best = (*best).min(elapsed);
            }
        }
        let ratio = long.as_secs_f64() / short.as_secs_f64();
        eprintln!(
            "linear-time {label}: round {round}, {LINEAR_GROWTH} x{short_n} {short:?}, \
             x{n} {long:?}, ratio {ratio:.2} (limit {LINEAR_RATIO_LIMIT})"
        );
        if ratio < LINEAR_RATIO_LIMIT {
            return long_out;
        }
        rounds.push(format!("{short:?} and {long:?}, {ratio:.2}"));
    }
    panic!(
        "{label}: {LINEAR_GROWTH} lines of x{short_n} against one of x{n}, in each of \
         {LINEAR_ROUNDS} rounds: {}; a linear render is about 1 times as long and the \
         limit is {LINEAR_RATIO_LIMIT}",
        rounds.join("; ")
    );
}

fn newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&byte| byte == b'\n').count()
}

/// CPU time this thread has spent, for the linear-time tests.
///
/// Wall time also counts every moment another process held the core, so on
/// a loaded machine a timing failed with the algorithm unchanged; the
/// thread's own CPU time counts only the work the claim is about. It still
/// runs slower on a slower or busier core, which is why
/// [`assert_renders_in_linear_time`] compares two sizes instead of holding
/// one to a fixed budget.
fn thread_cpu_time() -> std::time::Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid, writable timespec for the call's duration.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    assert_eq!(status, 0, "the thread CPU clock is readable");
    std::time::Duration::new(
        u64::try_from(now.tv_sec).expect("a thread's CPU time is not negative"),
        u32::try_from(now.tv_nsec).expect("nanoseconds fit"),
    )
}

/// The rendered bytes as text; every expected value in these tests is UTF-8.
fn text(out: &[u8]) -> &str {
    std::str::from_utf8(out).expect("presenter output is UTF-8")
}

fn contains(out: &[u8], needle: &str) -> bool {
    text(out).contains(needle)
}

/// Only the thematic rule completion, which also enables Setext headings and
/// definition lists.
fn rule_only<'a>(rule: &'a mut ThematicRuleCompletion<'a>) -> MarkdownCompletions<'a> {
    MarkdownCompletions {
        thematic_rule: Some(rule),
        ..MarkdownCompletions::default()
    }
}

/// Only the code block completion.
fn code_only<'a>(code: &'a mut CodeBlockCompletion<'a>) -> MarkdownCompletions<'a> {
    MarkdownCompletions {
        code: Some(code),
        ..MarkdownCompletions::default()
    }
}
