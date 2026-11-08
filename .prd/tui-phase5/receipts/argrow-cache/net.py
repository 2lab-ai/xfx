#!/usr/bin/env python3
"""ARGROW-CACHE: train 6's net.py (join check 4), run on this unit's new and
changed tests, by name: each with every
https and http proxy variable naming a loopback logger on an ephemeral port
(FLAKE-CATALOG-EVENT's connect_logger.py: it logs each connection's first
request line, answers 403 and forwards nothing). The logger is this script's
own child, started with Popen and ended by it.

    python3 -I net.py <clone> <connect_logger.py> <out-dir>
"""
import os
import subprocess
import sys
import time

clone, logger, out = sys.argv[1:4]
os.makedirs(out, exist_ok=True)
log = os.path.join(out, "connect.log")
open(log, "w").close()
child = subprocess.Popen([sys.executable, "-I", logger, "0", log, "10800"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(100):
    if os.path.exists(log + ".port"):
        break
    time.sleep(0.1)
port = open(log + ".port").read().strip()
proxy = f"http://127.0.0.1:{port}"
env = {k: v for k, v in os.environ.items()
       if k not in ("NO_PROXY", "no_proxy", "ALL_PROXY", "all_proxy")}
env.update(HTTPS_PROXY=proxy, https_proxy=proxy, HTTP_PROXY=proxy, http_proxy=proxy)
print(f"# logger pid {child.pid}, 127.0.0.1:<ephemeral>; HTTPS_PROXY, https_proxy, HTTP_PROXY, "
      f"http_proxy at it; NO_PROXY, no_proxy, ALL_PROXY, all_proxy unset", flush=True)
try:
    tests = [
        "tui::full_transcript::tests::ctrl_o_wraps_a_calls_arguments_once_for_a_width_and_not_again_for_a_join_or_a_theme",
        "tui::shell::tests::ctrl_o_with_arguments_through_its_caches_is_the_session_laid_out_from_nothing_after_every_event",
        "tui::full_transcript::tests::rows_kept_unpainted_are_the_rows_railed_painted_at_once",
        "tui::wrap::tests::painting_reads_the_clusters_graphemes_reads",
        "tui::wrap::tests::a_printable_ascii_character_is_one_cell",
        "tui::wrap::tests::a_run_stepped_over_is_the_clusters_graphemes_reads_there",
        "tui::prose::tests::stepping_over_what_the_wrap_only_counts_changes_no_row",
        "tui::prose::tests::a_wrap_of_megabytes_of_one_token_steps_a_row_at_a_time",
        "tui::full_transcript::tool_status::tests::counting_a_run_at_once_changes_no_break",
    ]
    for name, args in (("new", ["cargo", "test", "--locked", "--lib", "--no-fail-fast", "--", "--exact", *tests]),):
        before = sum(1 for _ in open(log))
        started = time.monotonic()
        with open(os.path.join(out, f"net-{name}.log"), "w") as f:
            rc = subprocess.run(args, cwd=clone, env=env, stdout=f, stderr=subprocess.STDOUT).returncode
        lines = open(log).read().splitlines()[before:]
        results = [l for l in open(os.path.join(out, f"net-{name}.log")) if l.startswith("test result:")]
        print(f"## {name}: `{' '.join(args)}` exit {rc}, {time.monotonic() - started:.0f} s", flush=True)
        for r in results:
            print("   " + r.strip())
        print(f"   connections logged during it: {len(lines)}")
        for l in sorted(set(lines)):
            print(f"     {lines.count(l)} x {l}")
finally:
    child.terminate()
    child.wait(timeout=10)
    print(f"# logger {child.pid} ended ({child.returncode})")
