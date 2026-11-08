#!/usr/bin/env python3
"""MODELS-REASK round-2 mutants: each a one-place change of a rule the unit added,
applied to a throwaway `git clone --no-local` of the code head, the lib tests
built and run filtered to the catalog's cases, under a per-mutant timeout,
the clone restored (`git checkout -- .`) after each. A mutant that hangs past
its timeout is KILLED (hang).

usage: mutants.py <clone> <out>
"""
import subprocess, sys, time, os, signal

clone, out = sys.argv[1], sys.argv[2]
FILTERS = ["catalog", "backoff", "model", "column", "reask", "refresh", "retry", "revalidat", "hold", "runtime", "image", "due", "answer", "slow_read", "historys"]
TIMEOUT = 900

M = [
 ("V-no-runtime-untold", "src/provider/model.rs",
  "            Err(err) => return Self::unstarted(view, done, told, finished, &err),",
  "            Err(_) => return Self { view, done, stop: None, thread: None, unstarted: None },"),
 ("W-midwork-unstarted-waits-for-the-work", "src/tui/worker.rs",
  "                let unstarted = load.answer_unstarted();",
  "                let unstarted: Option<(crate::provider::model::Loaded, Instant)> = None;"),
 ("X-refresh-failure-keeps-the-latch", "src/tui/shell.rs",
  "                self.catalog_nudged = false;\n                if let CatalogKnown::Unread(failed, _) = self.catalog_known {",
  "                if let CatalogKnown::Unread(failed, _) = self.catalog_known {"),
 ("Y-unread-hold-not-moved", "src/tui/shell.rs",
  "                    self.catalog_known = CatalogKnown::Unread(failed, due);",
  "                    let _ = (failed, due);"),
 ("Z1-rows-due-from-receipt", "src/tui/shell.rs",
  "                self.forget_catalog_clock();\n                self.catalog_due = Some(due);",
  "                self.forget_catalog_clock();\n                let _ = due;\n                self.catalog_due = Some(Instant::now() + crate::provider::model::REFRESH_INTERVAL);"),
 ("Z2-refresh-due-from-receipt", "src/tui/shell.rs",
  "                self.catalog_note = retry;\n                self.catalog_due = Some(due);",
  "                self.catalog_note = retry;\n                self.catalog_due = Some(match retry { Some(_) => due, None => due.max(Instant::now() + crate::provider::model::REFRESH_INTERVAL) });"),
 ("Z3-told-clock-from-answer", "src/tui/worker.rs",
  "    let applied = Applied::of(held, clock, answer, began, answered);",
  "    let applied = Applied::of(held, clock, answer, answered, answered);"),
 ("Z4-activity-due-ignores-hold", "src/provider/model.rs",
  "        [self.not_before, aged]",
  "        [None, aged]"),
 ("Z5-activity-due-ages-a-retryable-refresh", "src/provider/model.rs",
  "            Held::Rows { retry_due: false } => self.began.map(|began| began + REFRESH_INTERVAL),\n            Held::Rows { retry_due: true } | Held::Nothing | Held::Failed => None,",
  "            Held::Rows { .. } => self.began.map(|began| began + REFRESH_INTERVAL),\n            Held::Nothing | Held::Failed => None,"),
 ("Z6-activity-due-otherwise-later", "src/provider/model.rs",
  "            .unwrap_or(otherwise)",
  "            .unwrap_or(otherwise + REFRESH_INTERVAL)"),
 ("Z7-refresh-failure-due-ignores-retry", "src/tui/worker.rs",
  "                Held::Rows {\n                    retry_due: applied.refresh_retry.is_some(),\n                },",
  "                Held::Rows { retry_due: true },"),
 ("Z8-held-rows-due-ignores-refresh-retry", "src/tui/worker.rs",
  "                Held::Rows {\n                    retry_due: refresh_retry.is_some(),\n                },",
  "                Held::Rows { retry_due: false },"),
 ("AA-tool-images-never-wait", "src/tui/worker.rs",
  "            && !self.asked.replace(true)\n            && selector.retrying()",
  "            && !self.asked.replace(true)\n            && false"),
 ("AB-tool-images-wait-every-request", "src/tui/worker.rs",
  "            && !self.asked.replace(true)",
  "            && { self.asked.set(true); true }"),
 ("AC-tool-gate-forgets-the-prompts-images", "src/tui/worker.rs",
  "        asked: std::cell::Cell::new(carries_images),",
  "        asked: std::cell::Cell::new(false),"),
 ("AD-prompt-images-no-wait", "src/tui/worker.rs",
  "    if state.selector.entry(state.selector.model()).is_none() {\n        await_failed_catalog_retry(state, turn).await;\n    }",
  "    if false {\n        await_failed_catalog_retry(state, turn).await;\n    }"),
 ("AE-turn-start-waits-for-the-historys-images", "src/tui/worker.rs",
  "    if !images.is_empty() && !request_needs_catalog(state) {",
  "    if carries_images && !request_needs_catalog(state) {"),
]

def run(cmd, timeout, log):
    p = subprocess.Popen(cmd, cwd=clone, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True, text=True)
    try:
        o, _ = p.communicate(timeout=timeout)
        return p.returncode, o, False
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        o, _ = p.communicate()
        return None, o or "", True

env_tmp = os.environ.get("TMPDIR", "")
with open(out, "w") as f:
    f.write(f"# MODELS-REASK mutants on a git clone --no-local of {subprocess.check_output(['git','-C',clone,'rev-parse','--short','HEAD'],text=True).strip()} (the code head)\n")
    f.write(f"# each: the change applied, `cargo test --locked --lib -- {' '.join(FILTERS)}` (timeout {TIMEOUT}s), FAILED lines and the result line, then `git checkout -- .`\n")
    only = [n for n in os.environ.get("MUTANTS_ONLY", "").split(",") if n]
    for name, path, old, new in [("unmutated", None, None, None)] + [m for m in M if not only or m[0] in only]:
        f.write(f"## {name}\n"); f.flush()
        if path:
            p = os.path.join(clone, path)
            s = open(p).read()
            if s.count(old) != 1:
                f.write(f"anchor not found once ({s.count(old)}): NOT RUN\n"); continue
            open(p, "w").write(s.replace(old, new))
            f.write(f"applied {name} in {path}\n")
        t = time.time()
        rc, o, hung = run(["cargo", "test", "--locked", "--lib", "--"] + FILTERS, TIMEOUT, f)
        lines = [l for l in o.splitlines() if l.startswith("test ") and l.endswith("FAILED") or l.startswith("test result") or l.startswith("error[")]
        for l in lines[:40]:
            f.write(l + "\n")
        verdict = "KILLED (hang)" if hung else ("SURVIVED" if rc == 0 else ("KILLED" if any(l.endswith("FAILED") for l in lines) else f"KILLED (did not build, rc={rc})"))
        if name == "unmutated":
            verdict = "baseline ok" if rc == 0 else f"BASELINE FAILED rc={rc}"
        f.write(f"=> {verdict} ({int(time.time()-t)}s)\n"); f.flush()
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
