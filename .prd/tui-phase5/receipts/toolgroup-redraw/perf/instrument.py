"""Adds, to a checkout's src (throwaway clone only), the counters the
hydration measurement reads, and the measurement itself (an event-loop test).

usage: python3 -I instrument.py <repo>
"""
import pathlib, re, sys
R = pathlib.Path(sys.argv[1])
def edit(path, old, new):
    p = R / path
    s = p.read_text()
    assert s.count(old) == 1, (path, old[:60], s.count(old))
    p.write_text(s.replace(old, new))

# 1. toolgroup: count rows() and highlighted() calls.
edit("src/tui/toolgroup.rs",
     "    fn rows(&self, palette: Palette) -> Vec<String> {\n",
     "    fn rows(&self, palette: Palette) -> Vec<String> {\n        ROWS_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n")
edit("src/tui/toolgroup.rs",
     "fn highlighted(command: &str, base: &str, palette: Palette) -> String {\n",
     "pub(crate) static ROWS_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\npub(crate) static HIGHLIGHT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\nfn highlighted(command: &str, base: &str, palette: Palette) -> String {\n    HIGHLIGHT_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n")

# 2. transcript: what the queue holds.
t = (R / "src/tui/transcript.rs").read_text()
block_arm = "Op::Block(rows, _, redraws, _)" if "Op::Block(rows, kinds, redraws, painted)" in t else ("Op::Block(rows, _, redraws)" if "Op::Block(rows, kinds, redraws)" in t else "Op::Block(rows, _)")
redraw_bytes = "redraws.iter().flatten().map(|r| r.drawn(super::theme::Mode::Dark).len() + r.drawn(super::theme::Mode::Light).len()).sum::<usize>()" if "redraws" in block_arm else "0"
edit("src/tui/transcript.rs",
     "    /// Queues a change of gutter for the lines that follow.",
     "    /// (operations, blocks, bytes of rows in blocks, bytes of drawings in blocks)\n    pub(crate) fn queued_measure(&self) -> (usize, usize, usize, usize) {\n        let mut m = (self.queue.len(), 0, 0, 0);\n        for op in &self.queue {\n            if let %s = op {\n                m.1 += 1;\n                m.2 += rows.iter().map(String::len).sum::<usize>();\n                m.3 += %s;\n            }\n        }\n        m\n    }\n\n    /// Queues a change of gutter for the lines that follow." % (block_arm, redraw_bytes))

# 3. shell: expose it.
edit("src/tui/shell.rs",
     "    /// The palette the band and the document are painted in now.\n",
     "    pub(crate) fn queued_measure(&self) -> (usize, usize, usize, usize) {\n        self.transcript.queued_measure()\n    }\n\n    /// The palette the band and the document are painted in now.\n")

# 4. the measurement.
edit("src/tui/event_loop.rs",
     "    #[test]\n    fn a_resumed_answers_code_block_is_recorded_and_recoloured_as_a_live_ones() {",
     '''    #[test]
    fn perf_measure_hydration() {
        use super::super::hydration::{Ending, HydratedStep, HydratedTurn, Hydration};
        use std::sync::atomic::Ordering::Relaxed;
        for (turns, calls) in [(20usize, 10usize), (20, 50), (5, 200)] {
            let mut shell = shell();
            let mut band = Band::new();
            let mut failures = FrameFailures::default();
            let mut out = screen();
            let now = Instant::now();
            tick(&mut shell, &mut band, &mut out, &mut failures, now);
            let shown = crate::tools::terminal::TERMINAL.shown(&serde_json::json!({
                "action": "exec",
                "command": "echo \\"hi\\" 42 | grep hi # note",
            }));
            let turns_v: Vec<HydratedTurn> = (0..turns)
                .map(|t| {
                    let mut steps = Vec::new();
                    for c in 0..calls {
                        let id = format!("{t}-{c}");
                        steps.push(HydratedStep::ToolStart { id: id.clone(), shown: shown.clone() });
                        steps.push(HydratedStep::ToolResult {
                            id,
                            ok: true,
                            outcome: crate::tools::Outcome::Exited(0),
                            changes: None,
                        });
                    }
                    steps.push(HydratedStep::Text("Done with it.\\n".to_string()));
                    HydratedTurn {
                        sent: format!("turn {t}"),
                        steps,
                        ending: Ending::Concluded { seconds: 0, output: Some(40) },
                        started_at_ms: 0,
                        concluded_at_ms: None,
                        details: Vec::new(),
                        images: Vec::new(),
                    }
                })
                .collect();
            super::super::toolgroup::ROWS_CALLS.store(0, Relaxed);
            super::super::toolgroup::HIGHLIGHT_CALLS.store(0, Relaxed);
            let started = std::time::Instant::now();
            shell.hydrate(
                Hydration {
                    title: Some("saved".to_string()),
                    turns: turns_v,
                    model: None,
                    settings: None,
                    keeps_fast_binding: true,
                    provider: None,
                    unauthenticated: None,
                    model_replaced: false,
                    next_image: 1,
                },
                false,
            );
            shell.flush_paced();
            let hydrate = started.elapsed();
            let queued = shell.queued_measure();
            let rows_calls = super::super::toolgroup::ROWS_CALLS.load(Relaxed);
            let highlights = super::super::toolgroup::HIGHLIGHT_CALLS.load(Relaxed);
            let started = std::time::Instant::now();
            out.written.clear();
            commit_frame(&mut shell, &mut band, &mut out, &mut failures, now, Reconciled).expect("drain");
            let drain = started.elapsed();
            let left = shell.queued_measure();
            eprintln!(
                "PERF turns={turns} calls={calls}: hydrate {:.1} ms; queued ops {} blocks {} row-bytes {} drawing-bytes {}; rows() calls {rows_calls}, highlight calls {highlights}; drain {:.1} ms, {} bytes written; left {:?}",
                hydrate.as_secs_f64() * 1e3, queued.0, queued.1, queued.2, queued.3,
                drain.as_secs_f64() * 1e3, out.written.len(), left
            );
        }
    }

    #[test]
    fn a_resumed_answers_code_block_is_recorded_and_recoloured_as_a_live_ones() {''')
print("instrumented", block_arm)
