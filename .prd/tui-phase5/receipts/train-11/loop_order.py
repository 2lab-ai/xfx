#!/usr/bin/env python3
"""Train 11, join check 3 (CATALOG-REFRESH x PASTE-GAP): the event loop's
turn order at each commit given, read from the code outside the tests
modules.

1. `session`'s loop: the order of its calls (collect_facts, read_burst,
   settle_input, take_ui_events, settle_band, collect_facts, commit_counted).
2. Every call into the decoder (`self.decoder.<x>`) in shell.rs, by the
   function that holds it.
3. Every caller of those functions (route_read, end_delivery,
   end_delivery_at, still_at, settle_input, plane_names_the_hold) under src/.
4. Every call that builds a frame (commit_counted, commit_tick, commit_frame,
   paint_alternate, flushed) in event_loop.rs, by the function that holds it.
5. Where the catalog's work is in the loop: every `catalog_now(` and
   `settle_column(` outside tests under src/tui/, by function.

    python3 -I -B loop_order.py <repo> <commit>...
"""
import re
import subprocess
import sys

FN = re.compile(r"^\s*(?:pub\(crate\) |pub )?(?:async )?fn ([a-z_0-9]+)")


def show(repo, commit, path):
    r = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True)
    return r.stdout.split("\n") if r.returncode == 0 else []


def files(repo, commit):
    out = subprocess.run(["git", "-C", repo, "ls-tree", "-r", "--name-only", commit, "src/"], check=True,
                         capture_output=True, text=True).stdout.split()
    return [f for f in out if f.endswith(".rs")]


def code(lines):
    end = next((i for i, l in enumerate(lines) if re.match(r"^mod tests \{", l)), len(lines))
    fn = None
    for i, l in enumerate(lines[:end]):
        m = FN.match(l)
        if m:
            # `#[cfg(test)]` among the attributes just above it: a test's helper.
            above = [x.strip() for x in lines[max(0, i - 3):i]]
            fn = m.group(1) + (" [cfg(test)]" if "#[cfg(test)]" in above else "")
        if l.strip().startswith("//"):
            continue
        yield i + 1, fn, l


def main():
    repo, commits = sys.argv[1], sys.argv[2:]
    for c in commits:
        print(f"== {c}")
        el = show(repo, c, "src/tui/event_loop.rs")
        order = [(n, re.search(r"(collect_facts|read_burst|settle_input|take_ui_events|settle_band|commit_counted)\(", l).group(1))
                 for n, fn, l in code(el) if fn and fn.split()[0] == "session"
                 and re.search(r"(collect_facts|read_burst|settle_input|take_ui_events|settle_band|commit_counted)\(", l)]
        print("1. session's loop: " + " -> ".join(f"{name} (:{n})" for n, name in order))
        dec = {}
        for n, fn, l in code(show(repo, c, "src/tui/shell.rs")):
            for m in re.finditer(r"self\.decoder\.([a-z_]+)", l):
                dec.setdefault(fn, set()).add(m.group(1))
        print("2. the decoder's calls, by function: " + "; ".join(f"{k}: {', '.join(sorted(v))}" for k, v in dec.items()))
        names = ["route_read", "end_delivery_at", "end_delivery", "still_at", "settle_input", "plane_names_the_hold"]
        callers = []
        for path in files(repo, c):
            for n, fn, l in code(show(repo, c, path)):
                for name in names:
                    if re.search(r"\." + name + r"\(", l):
                        callers.append(f"{path}:{n} {fn} -> {name}")
        print(f"3. their callers outside tests ({len(callers)}):")
        for x in callers:
            print("     " + x)
        frames = [f"{n} {fn} -> {m.group(1)}" for n, fn, l in code(el)
                  for m in [re.search(r"\b(commit_counted|commit_tick|commit_frame|paint_alternate|flushed)\(", l)]
                  if m and not re.match(r"^\s*(?:pub\(crate\) )?fn ", l)]
        print(f"4. frames built outside tests in event_loop.rs ({len(frames)}): " + "; ".join(frames))
        cat = []
        for path in [f for f in files(repo, c) if f.startswith("src/tui/")]:
            for n, fn, l in code(show(repo, c, path)):
                if re.search(r"\b(catalog_now|settle_column)\(", l) and not re.match(r"^\s*(?:pub\(crate\) )?fn ", l):
                    cat.append(f"{path}:{n} {fn}")
        print(f"5. the catalog's clock and column outside tests ({len(cat)}): " + "; ".join(cat))


main()
