#!/usr/bin/env python3
"""Train 11, join check 5: every `Runtime::new(` and
`Environment::from_process(` in the tests modules of the runtime's files at
<head>, with the test function that holds it and the commit `git blame`
names for that line; then, for each such commit, whether main holds it --
a line no unit wrote. (`Runtime::new` takes the process's environment, so a
test that builds one and reaches a reload reads the real profile; the common
brief's rule.)

    python3 -I -B runtime_new.py <repo> <head> <main>
"""
import re
import subprocess
import sys

repo, head, main = sys.argv[1:4]
FILES = ["src/tui/worker.rs", "src/tui/shell.rs", "src/tui/event_loop.rs", "src/provider/model.rs",
         "src/interactive.rs", "src/tui/bridge.rs", "src/provider/catalog_clock.rs", "src/config.rs"]
writers, n = set(), 0
for path in FILES:
    r = subprocess.run(["git", "-C", repo, "show", f"{head}:{path}"], capture_output=True, text=True)
    t = r.stdout.split("\n")
    start = next((i for i, l in enumerate(t) if re.match(r"^mod tests \{", l)), None)
    if start is None:
        continue
    fn = None
    for i in range(start, len(t)):
        m = re.match(r"^\s*(?:pub\(crate\) )?(?:async )?fn ([a-z_0-9]+)", t[i])
        if m:
            fn = m.group(1)
        if re.search(r"\bRuntime::new\(|Environment::from_process\(", t[i]) and not t[i].strip().startswith("//"):
            b = subprocess.run(["git", "-C", repo, "blame", "--porcelain", "-L", f"{i + 1},{i + 1}", head, "--", path],
                               capture_output=True, text=True).stdout.split()[0][:8]
            writers.add(b)
            n += 1
            print(f"{path}:{i + 1} in {fn}: {t[i].strip()[:80]}  [written by {b}]")
on_main = sum(subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", w, main]).returncode == 0 for w in writers)
print(f"# {n} lines, written by {len(writers)} commits; {on_main} of those commits on main {main[:8]}")
