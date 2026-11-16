#!/usr/bin/env python3
"""This unit's tests and META-PREFIX-DECODE's, read out of full `cargo test`
logs (the gate's plain and fault-injection runs; the run with rustup's
proxies first): each test's verdict in each. MPD's names are its receipt's
(`meta-prefix-decode/green.txt`, the plain section). A name missing from a
log is printed MISSING.

    python3 -I -B touched.py <receipts-dir> <label>=<log>...
Prints no path.
"""
import re
import sys

receipts, runs = sys.argv[1], [a.split("=", 1) for a in sys.argv[2:]]
MINE = [
    "tui::event_loop::tests::a_turn_read_whole_in_one_drain_is_working_after_a_frame_that_shows_its_thinking",
    "tui::event_loop::tests::keys_between_a_split_turns_start_and_its_queued_end_do_what_they_do_when_the_end_is_late",
    "tui::event_loop::tests::under_a_sustained_stream_a_split_turn_stops_the_drain_once_and_every_tick_reads_its_keys",
    "tui::event_loop::tests::the_drain_reads_past_a_start_that_owes_no_frame",
    "tui::event_loop::tests::the_drain_stops_behind_a_start_only_and_not_on_a_tick_whose_keys_owe_a_report",
    "tui::event_loop::tests::an_escape_pair_settled_in_the_tick_that_reads_a_held_prompts_start_leaves_the_start_ending_the_drain",
    "tui::shell::tests::a_turn_start_owes_its_frame_exactly_when_its_tick_writes_working_after_the_frame",
    "tui::input::tests::a_quarantine_ends_only_after_two_empty_checks_a_quiet_period_apart",
    "tui::input::tests::a_quarantine_ends_at_the_first_empty_check_at_or_past_the_quiet",
    "the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval",
    "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing",
    "the_harness_measures_a_pause_between_two_writes_of_one_paste",
    "an_unpaced_case_runs_a_new_session_only_after_a_paste_that_paused",
    "an_unpaced_case_whose_every_paste_paused_fails_saying_so",
    "the_last_bytes_of_a_stream_start_at_a_character",
    "a_skills_sandbox_path_never_holds_what_its_case_searches",
    "slash_skills_lists_installs_from_a_local_directory_and_never_from_the_network",
    "ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes",
]
text = open(f"{receipts}/meta-prefix-decode/green.txt").read()
section = text.split("## plain", 1)[1].split("##", 1)[0]
MPD = re.findall(r"^test (\S+) \.\.\. ok$", section, re.M)


def verdicts(path):
    out = {}
    for line in open(path, errors="replace"):
        m = re.match(r"^test (\S+) \.\.\. (ok|FAILED|ignored.*)$", line.rstrip("\n"))
        if m:
            out.setdefault(m.group(1), []).append(m.group(2))
    return out


logs = {label: verdicts(path) for label, path in runs}
bad = 0
for group, names in (("this unit", MINE), ("META-PREFIX-DECODE", MPD)):
    for name in names:
        cells = []
        for run, found in logs.items():
            v = found.get(name)
            cells.append(f"{run} {'MISSING' if not v else ','.join(v)}")
            bad += not v or any(x != "ok" for x in v)
        print(f"{group}: {name}: " + "; ".join(cells))
print(f"# {len(MINE)} of this unit's and {len(MPD)} of META-PREFIX-DECODE's, in {', '.join(logs)}; not ok or missing: {bad}")
sys.exit(1 if bad else 0)
