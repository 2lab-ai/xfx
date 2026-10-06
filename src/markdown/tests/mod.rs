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

use super::payload::{CodeBlockCompletion, ThematicRuleCompletion};
use super::MarkdownCompletions;

mod blocks;
mod code_tables;
mod events;
mod inline;
mod links;
mod setext_footnotes;

/// CPU time this thread has spent, for the linear-time tests.
///
/// Those tests assert that a 64 KiB line renders well inside a limit a
/// quadratic walk would blow by orders of magnitude. Wall time also counts
/// every moment another process held the core, so on a loaded machine the
/// limit failed with the algorithm unchanged; the thread's own CPU time counts
/// only the work the claim is about.
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
