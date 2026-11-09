#!/usr/bin/env python3
"""CATALOG-BOUNDS round 2 mutants: the llmux mapping and fakegw's own
bounds. Each is one edit on a throwaway `git clone --no-local` of the branch
(never the worktree), its tests run under ../bounded.py (300 s, 3 GiB) after
the crate is built outside it, the edit undone with `git checkout -- src
scripts`. Killed = a test named as failing.

usage: python3 -I mutants.py <clone> <cargo target dir> [<name substring>...]
Paths are arguments, so none of this machine is written here; TMPDIR comes
from the environment. Run it under the suite lock."""
import os, subprocess, sys

HERE = os.path.dirname(os.path.realpath(__file__))
BOUNDED = os.path.join(HERE, "..", "bounded.py")
MODEL, FAKEGW = "src/provider/model.rs", "scripts/fxcap/fakegw.ts"
RUST = ["cargo", "test", "--locked", "--lib", "--", "--test-threads=2", "a_llmux_catalog_", "llmux::setup"]
HARNESS = ["scripts/fxcap/test-capture.sh",
           "-k", "test_modelspad_and_modelsstall_out_of", "-k", "test_a_catalog_longer_than",
           "-k", "test_modelspad_and_modelsstall"]
MUTANTS = [
    ("llmux: no document is unavailable", MODEL, RUST,
     "LoopbackFailure::NotADocument(detail) => CatalogError::Malformed { detail },",
     "LoopbackFailure::NotADocument(detail) => CatalogError::Unavailable { detail },"),
    ("llmux: too large is no answer", MODEL, RUST,
     "CappedRead::TooLarge => NotADocument(larger_than(max_bytes)),",
     "CappedRead::TooLarge => NoAnswer(larger_than(max_bytes)),"),
    ("llmux: not UTF-8 is no answer", MODEL, RUST,
     'String::from_utf8(body).map_err(|_| NotADocument("the answer is not UTF-8 text".to_string()))',
     'String::from_utf8(body).map_err(|_| NoAnswer("the answer is not UTF-8 text".to_string()))'),
    ("llmux: a body cut short is no document", MODEL, RUST,
     "CappedRead::Read(err) => NoAnswer(err.to_string()),",
     "CappedRead::Read(err) => NotADocument(err.to_string()),"),
    ("fakegw: no padding bound", FAKEGW, HARNESS,
     "if (!m || Number(m[1]) > MAX_PAD_BYTES)", "if (!m)"),
    ("fakegw: no stall bound", FAKEGW, HARNESS,
     "|| Number(value) > MAX_STALL_MS)", ")"),
    ("fakegw: a padding that is not a whole number", FAKEGW, HARNESS,
     "/^([0-9]{1,8}) (head|tail)$/", "/^([0-9.e-]{1,8}) (head|tail)$/"),
    ("fakegw: no start-up check of the catalog's size", FAKEGW, HARNESS,
     "Bun.file(process.env.FAKEGW_MODELS).size > modelsPad.size) {", "false) {"),
    ("fakegw: no request-time size check (the fill then throws, a generic 500 with no refused line)", FAKEGW, HARNESS,
     "    if (bytes.length > pad.size) {", "    if (false) {"),
]

def run(clone, target, cmd):
    env = dict(os.environ, CARGO_TARGET_DIR=target)
    return subprocess.run(["python3", "-I", BOUNDED, "300", "3072", "--", *cmd], cwd=clone, env=env,
                          capture_output=True, text=True)

def main():
    clone, target = sys.argv[1], sys.argv[2]
    killed = 0
    only = sys.argv[3:]
    chosen = [m for m in MUTANTS if not only or any(o in m[0] for o in only)]
    for name, path, cmd, old, new in chosen:
        file = os.path.join(clone, path)
        text = open(file).read()
        assert text.count(old) == 1, f"{name}: the edit's text is not in {path} once"
        open(file, "w").write(text.replace(old, new))
        try:
            if cmd is RUST:
                # Built first, outside the watchdog: compiling the crate
                # takes more than its 3 GiB, and that is not the code under
                # test (round 2's first run counted four builds as kills).
                env = dict(os.environ, CARGO_TARGET_DIR=target)
                build = subprocess.run(["cargo", "test", "--locked", "--lib", "--no-run"], cwd=clone, env=env,
                                       capture_output=True, text=True)
                if build.returncode != 0:
                    print(f"- {name} ({path}): does not build -- not counted", flush=True)
                    continue
            r = run(clone, target, cmd)
            out = r.stdout + r.stderr
            failed = [l.split()[1].split("::")[-1] for l in out.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            failed += [l.split()[1] for l in out.splitlines() if l.startswith(("FAIL: ", "ERROR: "))]
            if r.returncode == 0:
                verdict = "SURVIVED"
            elif not failed:
                # A non-zero exit with no failing test named (the watchdog,
                # a crash) says nothing about the edit.
                verdict = "INCONCLUSIVE: " + out.strip().splitlines()[-1]
            else:
                killed += 1
                verdict = "killed by " + ", ".join(failed)
        finally:
            subprocess.run(["git", "checkout", "--", "src", "scripts"], cwd=clone)
        print(f"- {name} ({path}): {verdict}", flush=True)
    print(f"{killed} of {len(chosen)} killed")
    return 0 if killed == len(chosen) else 1

sys.exit(main())
