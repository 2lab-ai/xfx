"""INPUT-ROW-LIMIT's mutants, each on a `git clone --no-local` throwaway of
the head and never on the worktree: the old `content_bottom` (`rows - 3`),
no `> 4` guard, and every surface that reads the limit left on the old one
(`(rows - 3) / 2 + 1` inlined at that one call), one at a time. For each, the
clone's tree is reset, the one edit made (exactly one match, or the run
stops), and the unit's tests run; a mutant is killed when a test fails.

usage: python3 -I mutants.py <clone> <out.txt>
"""

import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True

TESTS = ["at_41_rows", "on_an_eight_row_screen", "the_composer_may_take_half"]
OLD = "(self.geometry.rows.saturating_sub(3) / 2 + 1)"
CALL = "layout::input_row_limit(self.geometry.rows)"


def surfaces(text):
    """Every `CALL` in shell.rs outside its tests, named by the fn it is in,
    plus `fit`'s `layout::input_row_limit(rows)`."""
    body = text.split("#[cfg(test)]\nmod tests {")[0]
    found = []
    for m in re.finditer(re.escape(CALL), body):
        fn = re.findall(r"fn ([a-z_0-9]+)\(", body[: m.start()])[-1]
        found.append((fn, m.start()))
    fit = body.index("layout::input_row_limit(rows)")
    found.append(("fit", fit))
    return found


def mutants(clone):
    shell = open(f"{clone}/src/tui/shell.rs", encoding="utf-8").read()
    out = [
        ("old-content-bottom", "src/tui/layout.rs", "rows.saturating_sub(FX_FOOTER_ROWS)", "rows.saturating_sub(3)", None),
        (
            "no-guard",
            "src/tui/layout.rs",
            "let extra = if content_bottom > 4 {\n        content_bottom / 2\n    } else {\n        0\n    };",
            "let extra = content_bottom / 2;",
            None,
        ),
    ]
    for fn, at in surfaces(shell):
        if fn == "fit":
            out.append(("old-limit:fit", "src/tui/shell.rs", "layout::input_row_limit(rows)", "(rows.saturating_sub(3) / 2 + 1)", at))
        else:
            label = "act, the composer's page" if fn == "act" else fn
            out.append((f"old-limit:{label}", "src/tui/shell.rs", CALL, OLD, at))
    return out


def run(cmd, cwd, timeout=None):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    lines = []
    for name, path, old, new, at in mutants(clone):
        run(["git", "checkout", "--", "src"], clone)
        full = f"{clone}/{path}"
        text = open(full, encoding="utf-8").read()
        if at is None:
            assert text.count(old) == 1, (name, text.count(old))
            text = text.replace(old, new)
        else:
            assert text[at : at + len(old)] == old, name
            text = text[:at] + new + text[at + len(old) :]
        open(full, "w", encoding="utf-8").write(text)
        diff = run(["git", "diff", "--stat"], clone).stdout.strip().splitlines()[-1]
        start = time.time()
        res = run(["cargo", "test", "--locked", "--lib", "--", *TESTS], clone, timeout=1800)
        failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", res.stdout, re.M)))
        built = "test result:" in res.stdout
        verdict = "KILLED" if failed else ("SURVIVED" if built else "NO-BUILD")
        lines.append(f"{name}: {verdict} ({len(failed)} failed, {time.time() - start:.0f}s; {diff})")
        for f in failed:
            lines.append(f"    {f}")
        if not built:
            lines.append("    " + res.stderr.strip().splitlines()[-1] if res.stderr.strip() else "    (no output)")
        print(lines[-1 - len(failed)], flush=True)
    run(["git", "checkout", "--", "src"], clone)
    head = run(["git", "rev-parse", "--short", "HEAD"], clone).stdout.strip()
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(f"# mutants over the head {head} on a --no-local clone; tests: {' '.join(TESTS)}\n")
        f.write("\n".join(lines) + "\n")
    killed = sum(1 for l in lines if ": KILLED" in l)
    total = sum(1 for l in lines if not l.startswith("    "))
    print(f"killed {killed}/{total}")


if __name__ == "__main__":
    main()
