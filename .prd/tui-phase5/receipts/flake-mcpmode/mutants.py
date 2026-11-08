"""Half-reverts of the FLAKE-MCPMODE fix, each on a throwaway copy of the
final fix commit's scripts/fxcap; each must make a test fail.
usage: python3 mutants.py <frozen-fix-tree> <scratch-dir>
Each mutant's tests run under `timeout 300`: a mutant with no deadline
never answers, and its test's own client timeout is what fails it."""
import os, shutil, subprocess, sys

src, work = sys.argv[1], sys.argv[2]
MUTANTS = [
    ("mcphttp: one read, no retry", "mcphttp.ts",
     "    if (Date.now() >= until) return { now: `unknown(${word})`, readError };", "    return { now: `unknown(${word})`, readError };",
     ["McpHttp.test_a_held_request_reads_past_a_mode_file_caught_empty"]),
    ("mcphttp: no deadline (follow-up F)", "mcphttp.ts",
     "    if (Date.now() >= until) return { now: `unknown(${word})`, readError };\n", "",
     ["McpHttp.test_a_mode_file_that_never_holds_a_mode_is_answered_unknown_at_the_deadline"]),
    ("mcphttp: the read error not kept (follow-up F)", "mcphttp.ts",
     '      (e) => ((readError = String(e?.code ?? e?.name ?? "error")), ""),', '      (e) => "",',
     ["McpHttp.test_a_mode_file_that_cannot_be_read_is_answered_unknown_with_its_error"]),
    ("mcphttp: malformed and hostile held to the deadline (follow-up F)", "mcphttp.ts",
     '    if (modes.has(word)) return { now: word, readError: "" };',
     '    if (modes.has(word) && ((word !== "malformed" && word !== "hostile") || Date.now() >= until)) return { now: word, readError: "" };',
     ["McpHttp.test_malformed_and_hostile_are_modes_and_answer_without_the_wait"]),
    ("mcphttp: bound 0", "mcphttp.ts", "const MODE_SETTLE_MS = 2000;", "const MODE_SETTLE_MS = 0;",
     ["McpHttp.test_a_held_request_reads_past_a_mode_file_caught_empty"]),
    ("fakegw: a read that does not parse is taken at once", "fakegw.ts",
     "      if (Date.now() >= until) return text;", "      return text;",
     ["FakeGateway.test_a_usage_file_or_second_catalog_caught_empty_is_read_again"]),
    ("fakegw: no deadline (follow-up F)", "fakegw.ts",
     "      if (Date.now() >= until) return text;\n", "",
     ["FakeGateway.test_a_usage_file_that_never_parses_is_answered_500_at_the_deadline"]),
    ("fakegw: models2 read as before (exists, then text)", "fakegw.ts",
     "        const second = process.env.FAKEGW_MODELS2 ? await settled(process.env.FAKEGW_MODELS2) : null;",
     "        const second = process.env.FAKEGW_MODELS2 && (await Bun.file(process.env.FAKEGW_MODELS2).exists()) ? await Bun.file(process.env.FAKEGW_MODELS2).text() : null;",
     ["FakeGateway.test_a_usage_file_or_second_catalog_caught_empty_is_read_again"]),
    ("fakegw: ENOENT is a failed read, not absent", "fakegw.ts",
     '      if (e?.code === "ENOENT") return null;\n', "",
     ["Verbs.test_usage_is_in_place_at_launch_and_swapped_on_cue", "FakeGateway.test_the_usage_routes_their_shapes_and_knobs"]),
    ("capture.sh: clock in place", "capture.sh",
     '''clock) (umask 077 && printf '%s\\n' "$rest" > "$run/home.tmp"); put_in_home "$run/home.tmp" "$run/home/.fxcap-usage-clock" ;;''',
     '''clock) (umask 077 && printf '%s\\n' "$rest" > "$run/home/.fxcap-usage-clock") ;;''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
    ("capture.sh: reseed in place", "capture.sh",
     '''        put_in_home "$run/home.tmp" "$run/home/.$which/settings.json"''',
     '''        cat "$run/home.tmp" > "$run/home/.$which/settings.json"; rm -f "$run/home.tmp"''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
    ("capture.sh: rehomefile in place", "capture.sh",
     '''        put_in_home "$run/home.tmp" "$run/home/$dest"''',
     '''        cat "$run/home.tmp" > "$run/home/$dest"; rm -f "$run/home.tmp"''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole", "Verbs.test_rehomefile"]),
    ("capture.sh: put_in_home without its link/directory refusal", "capture.sh",
     '''  if [[ -L $dest || -d $dest ]]; then''',
     '''  if false; then''',
     ["Confinement.test_a_home_file_a_verb_replaces_is_never_a_link_or_a_directory"]),
    ("capture.sh: put_in_home refusing a directory only", "capture.sh",
     '''  if [[ -L $dest || -d $dest ]]; then''',
     '''  if [[ -d $dest && ! -L $dest ]]; then''',
     ["Confinement.test_a_home_file_a_verb_replaces_is_never_a_link_or_a_directory"]),
    ("capture.sh: release appended", "capture.sh",
     '''release) { cat "$run/gw-release" 2>/dev/null || true; echo release; } > "$run/gw-release.tmp"; mv -f "$run/gw-release.tmp" "$run/gw-release" ;;''',
     '''release) echo release >> "$run/gw-release" ;;''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
    ("capture.sh: clipimage copied in place", "capture.sh",
     '''*) cp "$scenarios/$rest" "$run/clipimage.tmp"; mv -f "$run/clipimage.tmp" "$run/clipimage" ;;''',
     '''*) cp "$scenarios/$rest" "$run/clipimage" ;;''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
    ("capture.sh: usage copied in place", "capture.sh",
     '''*) cp "$scenarios/$rest" "$run/usage.tmp"; mv -f "$run/usage.tmp" "$run/usage.json" ;;''',
     '''*) cp "$scenarios/$rest" "$run/usage.json" ;;''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
    ("capture.sh: mcphttp mode written in place", "capture.sh",
     '''mcphttp) printf '%s\\n' "$rest" > "$run/mcphttp.tmp"; mv -f "$run/mcphttp.tmp" "$run/mcphttp.mode" ;;''',
     '''mcphttp) printf '%s\\n' "$rest" > "$run/mcphttp.mode" ;;''',
     ["Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole"]),
]
env = dict(os.environ, TMPDIR="/private/tmp/claude-501/flake-mcpmode/t")
killed = 0
for name, file, old, new, tests in MUTANTS:
    tree = os.path.join(work, "mutant")
    shutil.rmtree(tree, True)
    shutil.copytree(src, tree)
    path = os.path.join(tree, "scripts", "fxcap", file)
    text = open(path).read()
    assert text.count(old) == 1, (name, text.count(old))
    open(path, "w").write(text.replace(old, new))
    r = subprocess.run(["timeout", "300", "python3", "-I", "scripts/fxcap/test_capture.py", *tests],
                       cwd=tree, env=env, capture_output=True, text=True)
    last = [l for l in r.stderr.splitlines() if l.startswith(("Ran", "OK", "FAILED"))]
    fails = sorted({l.split(" (")[0] for l in r.stderr.splitlines() if l.startswith(("FAIL:", "ERROR:"))})
    verdict = "KILLED" if r.returncode != 0 else "SURVIVED"
    killed += r.returncode != 0
    print("%-8s %-55s rc=%d %s | %s" % (verdict, name, r.returncode, " ".join(last), "; ".join(fails)))
    shutil.rmtree(tree, True)
print("killed %d/%d" % (killed, len(MUTANTS)))
