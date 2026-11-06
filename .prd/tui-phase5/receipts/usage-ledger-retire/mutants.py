"""US-U5 mutants, applied one at a time to the throwaway clone given as argv[1]."""
import json
import subprocess
import sys

clone = sys.argv[1]
only = sys.argv[2:] or None


def edit(path, old, new):
    full = f"{clone}/{path}"
    text = open(full).read()
    assert text.count(old) == 1, (path, old[:80], text.count(old))
    open(full, "w").write(text.replace(old, new))


MUTANTS = {
    "M1-completed-step-not-spent": (
        [("src/agent/machine.rs",
          "                Ok(Stepped::Completed(completion)) => {\n                    self.spent = true;\n",
          "                Ok(Stepped::Completed(completion)) => {\n")],
        [["--test", "tool_loop", "a_turn_that_failed_after_a_request_still_writes_its_summary_frame"]],
    ),
    "M2-compaction-not-spent": (
        [("src/agent/machine.rs", "        self.spent |= compactor.spent();\n", "        let _ = compactor.spent();\n")],
        [["--lib", "a_turn_a_compaction_spent_on_writes_its_frame_though_its_request_is_refused"]],
    ),
    "M3-frame-only-on-success": (
        [("src/agent/machine.rs", "        if result.is_ok() || self.spent {\n", "        if result.is_ok() {\n")],
        [["--test", "tool_loop", "a_turn_that_failed_after_a_request_still_writes_its_summary_frame"],
         ["--lib", "a_turn_a_compaction_spent_on_writes_its_frame_though_its_request_is_refused"]],
    ),
    "M4-lost-notes-request-not-spent": (
        [("src/agent/compact.rs",
          "            if may_have_billed(&err) {\n                caller.spent.asked.set(true);\n            }\n",
          "            let _ = may_have_billed(&err);\n")],
        [["--lib", "a_notes_request_answered_or_lost_is_spent_and_one_refused_is_not"]],
    ),
    "M5-variant-denies-unknown-fields": (
        [("src/session/event.rs",
          "#[serde(tag = \"kind\", rename_all = \"snake_case\")]\npub enum SessionEvent {",
          "#[serde(tag = \"kind\", rename_all = \"snake_case\", deny_unknown_fields)]\npub enum SessionEvent {")],
        [["--lib", "a_frame_the_ledger_wrote_decodes_and_encodes_without_the_ledger"],
         ["--lib", "a_session_the_ledger_build_wrote_replays_its_turns_and_totals"],
         ["--test", "ledger_compat", "the_ledger_builds_session_resumes_replays_and_appends_without_its_fields"]],
    ),
    "M6-binary-holds-the-bill-route": (
        [("src/main.rs", "fn main() ", "#[used]\nstatic GENERATION_PATH: &[u8; 14] = b\"/v1/generation\";\n\nfn main() ")],
        [["--test", "ledger_compat", "the_binary_holds_no_bill_route_and_no_ledger_file_name"]],
    ),
    "M7-commit-writes-usage-json": (
        [("src/session/store.rs",
          "        if let Err(err) = outcome {\n            self.failure = Some(format!(\"cannot record the `{kind}` session event: {err}\"));\n        }\n",
          "        if let Err(err) = outcome {\n            self.failure = Some(format!(\"cannot record the `{kind}` session event: {err}\"));\n        } else if kind == \"usage_recorded\" {\n            let path = self.store.profile_dir().join(\"usage.json\");\n            if std::fs::symlink_metadata(&path).is_err() {\n                let _ = std::fs::write(&path, \"{\\\"tracking_started_at_ms\\\":1}\\n\");\n            }\n        }\n")],
        [["--test", "sessions", "a_saved_turn_records_its_tokens_and_no_usage_ledger"],
         ["--test", "ledger_compat", "the_binary_holds_no_bill_route_and_no_ledger_file_name"]],
    ),
    "M8-commit-rewrites-an-old-usage-json": (
        [("src/session/store.rs",
          "        if let Err(err) = outcome {\n            self.failure = Some(format!(\"cannot record the `{kind}` session event: {err}\"));\n        }\n",
          "        if let Err(err) = outcome {\n            self.failure = Some(format!(\"cannot record the `{kind}` session event: {err}\"));\n        } else if kind == \"usage_recorded\" {\n            let path = self.store.profile_dir().join(\"usage.json\");\n            if std::fs::symlink_metadata(&path).is_ok() {\n                let _ = std::fs::write(&path, \"{\\\"tracking_started_at_ms\\\":1}\\n\");\n            }\n        }\n")],
        [["--test", "ledger_compat", "an_old_usage_json_is_left_as_it_was"]],
    ),
    "M9-lost-request-not-spent": (
        [("src/agent/machine.rs",
          "                        if may_have_billed(&err) {\n                            self.spent = true;\n                        }\n",
          "                        let _ = may_have_billed(&err);\n")],
        [["--test", "tool_loop", "a_turn_whose_only_request_was_lost_after_it_was_sent_writes_its_frame"]],
    ),
    "M10-steered-request-not-spent": (
        [("src/agent/machine.rs",
          "                    if sent {\n                        self.spent = true;\n                    }\n",
          "                    let _ = sent;\n")],
        [["--test", "tool_loop", "a_request_a_steer_cut_off_makes_a_refused_turns_frame_due"]],
    ),
}


def run(args):
    command = ["cargo", "test", "--locked", *args[:-1], "--", "--exact" if False else "--nocapture", args[-1]]
    done = subprocess.run(
        ["timeout", "600", *command],
        cwd=clone,
        capture_output=True,
        text=True,
    )
    out = done.stdout + done.stderr
    passed = [l for l in out.splitlines() if l.startswith("test ") and l.endswith(" ok")]
    failed = [l for l in out.splitlines() if l.startswith("test ") and "FAILED" in l]
    built = "error[" not in out and "could not compile" not in out
    return built, passed, failed, out


results = {}
for name, (edits, tests) in MUTANTS.items():
    if only and name not in only:
        continue
    subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
    for path, old, new in edits:
        edit(path, old, new)
    verdicts = []
    for test in tests:
        built, passed, failed, out = run(test)
        if not built:
            verdicts.append((test[-1], "BUILD-FAILED"))
            print(out[-3000:])
        elif failed:
            verdicts.append((test[-1], "KILLED"))
        elif passed:
            verdicts.append((test[-1], "SURVIVED"))
        else:
            verdicts.append((test[-1], "NOT-RUN"))
    results[name] = verdicts
    print(name, verdicts, flush=True)
    subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)

print(json.dumps(results, indent=1))
