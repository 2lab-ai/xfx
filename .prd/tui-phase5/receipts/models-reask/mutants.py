#!/usr/bin/env python3
"""MODELS-REASK mutants: each a one-place change of a rule the unit added,
applied to a throwaway `git clone --no-local` of the code head, the lib tests
built and run filtered to the catalog's cases, under a per-mutant timeout,
the clone restored (`git checkout -- .`) after each. A mutant that hangs past
its timeout is KILLED (hang).

usage: mutants.py <clone> <out>
"""
import subprocess, sys, time, os, signal

clone, out = sys.argv[1], sys.argv[2]
FILTERS = ["catalog", "backoff", "model", "column", "reask", "refresh", "retry", "revalidat", "hold"]
TIMEOUT = 900

M = [
 ("A-no-backoff", "src/provider/model.rs",
  "        if self.not_before.is_some_and(|until| now < until) {",
  "        if false && self.not_before.is_some_and(|until| now < until) {"),
 ("B-no-doubling", "src/provider/model.rs", "        delay *= 2;", "        delay *= 1;"),
 ("C-no-cap", "src/provider/model.rs", "    delay.min(FAILURE_BACKOFF_CAP)", "    delay"),
 ("D-picker-no-floor", "src/provider/model.rs",
  "                let picker_due = trigger == LoadTrigger::PickerOpen",
  "                let picker_due = trigger == LoadTrigger::PickerOpen || true"),
 ("E-never-stale", "src/provider/model.rs",
  "                let expired = elapsed.is_none_or(|elapsed| elapsed >= REFRESH_INTERVAL);",
  "                let expired = elapsed.is_none() && false;"),
 ("F-failed-refresh-drops-rows", "src/provider/model.rs",
  "            (CatalogState::Failed(_), retry) if rows_held => Self {",
  "            (CatalogState::Failed(_), retry) if rows_held && false => Self {"),
 ("G-backoff-from-start", "src/provider/model.rs",
  "            not_before: Some(answered + failure_backoff(streak)),",
  "            not_before: Some(began + failure_backoff(streak)),"),
 ("H-retry-hides-failure", "src/provider/model.rs",
  "        self.pending = Some(PendingLoad::start(fetcher, told));\n        true",
  "        self.catalog_state = CatalogState::NotLoaded;\n        self.pending = Some(PendingLoad::start(fetcher, told));\n        true"),
 ("I-menu-no-revalidate", "src/tui/shell.rs",
  "                self.work\n                    .control(TurnControl::Catalog(CatalogAsk::REVALIDATE));",
  "                let _ = CatalogAsk::REVALIDATE;"),
 ("J-menu-asks-inside-hold", "src/tui/shell.rs",
  "            CatalogKnown::Unread(retry, retry_at) if Instant::now() < retry_at => {",
  "            CatalogKnown::Unread(retry, retry_at) if Instant::now() < retry_at && false => {"),
 ("K-prompt-no-activity", "src/tui/shell.rs",
  "        let _ = self.note_catalog_activity(Instant::now());", "        let _ = 0;"),
 ("L-activity-ignores-hold", "src/tui/shell.rs",
  "            CatalogKnown::Unread(_, retry_at) => now >= retry_at,",
  "            CatalogKnown::Unread(_, _retry_at) => true,"),
 ("T-retry-shows-loading", "src/tui/shell.rs",
  "            .control(TurnControl::Catalog(CatalogAsk::ACTIVITY));\n        self.catalog_nudged = true;\n        true",
  "            .control(TurnControl::Catalog(CatalogAsk::ACTIVITY));\n        self.catalog_nudged = true;\n        if matches!(self.catalog_known, CatalogKnown::Unread(..)) { self.catalog_known = CatalogKnown::Asked; }\n        true"),
 ("U-unstartable-not-retryable", "src/provider/model.rs",
  "        Some(CatalogRetry::Runtime),\n    )",
  "        None,\n    )"),
 ("M-refresh-note-ignored", "src/tui/shell.rs",
  "                self.catalog_note = retry;", "                self.catalog_note = None;"),
 ("N-menu-resets-mark", "src/tui/model_menu.rs", "        if !was_ready {", "        if !was_ready || true {"),
 ("O-turn-no-retry-wait", "src/tui/worker.rs",
  "    if !state.selector.retrying() {", "    if true || !state.selector.retrying() {"),
 ("P-turn-unbounded-wait", "src/tui/worker.rs",
  "        _ = tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, state.selector.settle_load()) => {}",
  "        _ = state.selector.settle_load() => {}"),
 ("Q-work-asks-ignore-due", "src/tui/worker.rs",
  "            .is_some_and(|held| seen.clock.due(held, ask.trigger, now));",
  "            .is_some();"),
 ("R-idle-ask-ignores-trigger", "src/tui/worker.rs",
  "    if state.selector.would_load(ask.trigger, now) {",
  "    if state.selector.would_load(crate::provider::model::LoadTrigger::PickerOpen, now) {"),
 ("R2-idle-ask-reads-as-the-menu", "src/tui/worker.rs",
  "        state.selector.begin_load_on(ask.trigger, now, told);\n    }\n    if state.selector.is_loading() || !ask.awaited {",
  "        state.selector.begin_load_on(ask.trigger, now, told);\n    }\n    let told = telling(state.config.provider, events.clone(), state.selector.catalog().clone(), state.selector.clock());\n    state.selector.begin_load_on(crate::provider::model::LoadTrigger::PickerOpen, now, told);\n    if state.selector.is_loading() || !ask.awaited {"),
 ("S-old-sentence", "src/provider/model.rs",
  '            Self::Unreachable => "Could not reach AI Gateway; xfx retries automatically.",',
  '            Self::Unreachable => "Could not reach AI Gateway; retry /model.",'),
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
