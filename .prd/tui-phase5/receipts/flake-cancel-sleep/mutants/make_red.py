"""Applies the RED variants to a throwaway clone at the fix commit.

Every mutant is switched on by an environment variable, so one build carries
them all; the old tests are added beside the new ones as `old_...`.
"""
import re, subprocess, sys
clone = sys.argv[1]
BASE = "88c7a5be"

def edit(path, old, new, count=1):
    p = f"{clone}/{path}"
    s = open(p).read()
    assert s.count(old) == count, (path, old[:60], s.count(old))
    open(p, "w").write(s.replace(old, new))

def base_fn(path, name):
    """The whole `#[test]`/`#[tokio::test]` item named `name` at BASE."""
    s = subprocess.run(["git", "-C", clone, "show", f"{BASE}:{path}"],
                       capture_output=True, text=True, check=True).stdout
    lines = s.splitlines(keepends=True)
    start = next(i for i, l in enumerate(lines) if re.search(rf"\bfn {name}\(", l))
    # back up over the attribute line(s)
    while lines[start - 1].lstrip().startswith("#["):
        start -= 1
    end = start
    while lines[end].rstrip() != lines[start][: len(lines[start]) - len(lines[start].lstrip())] + "}":
        end += 1
    return "".join(lines[start : end + 1])

# --- mutants in the code -------------------------------------------------
edit("src/tools/terminal.rs",
     "        if cancel.is_cancelled() {\n            // Asked first,",
     "        if cancel.is_cancelled() && std::env::var_os(\"XFX_RED_IGNORE_CANCEL\").is_none() {\n            // Asked first,")
edit("src/agent/machine.rs",
     "        let mut checked = start == Start::Checked;\n        loop {\n",
     "        let mut checked = start == Start::Checked;\n"
     "        if let Some(ms) = std::env::var(\"XFX_RED_SLOW_TURN_MS\").ok().and_then(|v| v.parse::<u64>().ok()) {\n"
     "            std::thread::sleep(std::time::Duration::from_millis(ms));\n        }\n        loop {\n")
edit("src/usage/report.rs",
     "    cancel: &CancellationToken,\n) -> Outcome {\n    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()",
     "    cancel: &CancellationToken,\n) -> Outcome {\n"
     "    if let Some(ms) = std::env::var(\"XFX_RED_SLOW_FETCH_MS\").ok().and_then(|v| v.parse::<u64>().ok()) {\n"
     "        std::thread::sleep(std::time::Duration::from_millis(ms));\n    }\n"
     "    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()")
edit("src/usage/report.rs",
     "            () = cancel.cancelled() => return Outcome::Cancelled,",
     "            () = cancel.cancelled(), if std::env::var_os(\"XFX_RED_FETCH_IGNORE_CANCEL\").is_none() => return Outcome::Cancelled,")

# --- a slow shell, in the tests' own commands ------------------------------
edit("tests/permissions.rs",
     "fn stop_once_written(",
     "fn red_delay() -> String {\n    std::env::var(\"XFX_RED_SHELL_DELAY\").map(|d| format!(\"sleep {d}; \")).unwrap_or_default()\n}\n\nfn stop_once_written(")
edit("tests/permissions.rs",
     "json!({ \"action\": \"exec\", \"command\": command }),\n        );\n        let returned = Instant::now();",
     "json!({ \"action\": \"exec\", \"command\": format!(\"{}{command}\", red_delay()) }),\n        );\n        let returned = Instant::now();")

# --- the old tests, beside the new ------------------------------------------
old = base_fn("tests/permissions.rs", "a_stopped_command_is_asked_to_stop_and_the_screen_is_shown_how_it_ended")
old = old.replace("fn a_stopped_command", "fn old_a_stopped_command")
old = old.replace("json!({ \"action\": \"exec\", \"command\": command }),",
                  "json!({ \"action\": \"exec\", \"command\": format!(\"{}{command}\", red_delay()) }),")
assert "red_delay()" in old
p = f"{clone}/tests/permissions.rs"; open(p, "a").write("\n" + old)

old = base_fn("tests/tool_loop.rs", "a_command_the_person_stopped_leaves_the_request_that_asked_for_it_counted")
old = old.replace("fn a_command_the_person_stopped", "fn old_a_command_the_person_stopped")
p = f"{clone}/tests/tool_loop.rs"; open(p, "a").write("\n" + old)

# lib tests live in `mod tests`; insert the old ones before its last line.
olds = []
for name in ["a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body",
             "a_dropped_fetch_stops_and_lets_go_of_the_key"]:
    o = base_fn("src/usage/report.rs", name)
    o = o.replace(f"fn {name}", f"fn old_{name}")
    o = o.replace("let (origin, server) = dripping(&stop);", "let (origin, _drips, server) = dripping(&stop);")
    assert "_drips" in o
    olds.append(o)
p = f"{clone}/src/usage/report.rs"; s = open(p).read()
anchor = "    fn a_url_safe_base64_echo_is_caught_where_it_differs_from_standard() {"
assert s.count(anchor) == 1
i = s.index(anchor); i = s.rindex("    #[test]\n", 0, i)
s = s[:i] + "".join(o + "\n" for o in olds) + s[i:]
open(p, "w").write(s)
print("applied")
