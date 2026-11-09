#!/usr/bin/env python3
"""CATALOG-BOUNDS mutants: each one edit to the head's source, run on a
throwaway `git clone --no-local` of the branch (never the worktree), the
unit's tests run under bounded.py (300 s, 3 GiB), then the edit undone with
`git checkout -- src`. A mutant is killed when a test fails.

usage: python3 -I mutants.py <clone> <cargo target dir>
The clone and the target dir are arguments, so no path of this machine is
written here; run it under the suite lock."""
import os, subprocess, sys

HERE = os.path.dirname(os.path.realpath(__file__))
TESTS = [
    "a_gateway_catalog_", "a_llmux_catalog_past",
    "usage::report::tests::an_answer_over_1_mib_is_refused_without_being_read_whole",
    "credits::tests::an_answer_past_64_kib_or_cut_short_is_not_read_as_one",
]
GW, MODEL, CREDITS = "src/gateway/mod.rs", "src/provider/model.rs", "src/credits.rs"
MUTANTS = [
    ("no declared-length check", GW,
     "    if response\n        .content_length()\n        .is_some_and(|length| length > limit as u64)\n    {\n        return Err(CappedRead::TooLarge);\n    }\n", ""),
    ("an answer of exactly the cap refused", GW,
     "if chunk.len() > limit - body.len() {", "if chunk.len() >= limit - body.len() {"),
    ("no cap while it reads", GW,
     "        if chunk.len() > limit - body.len() {\n            return Err(CappedRead::TooLarge);\n        }\n", ""),
    ("a failed read keeps the bytes before it", GW,
     "while let Some(chunk) = response.chunk().await.map_err(CappedRead::Read)? {",
     "while let Ok(Some(chunk)) = response.chunk().await {"),
    ("the Gateway's cap is the old 1 MiB", MODEL,
     "const MAX_GATEWAY_CATALOG_BODY_BYTES: usize = 16 * 1024 * 1024;",
     "const MAX_GATEWAY_CATALOG_BODY_BYTES: usize = 1024 * 1024;"),
    ("a deadline mid-body is not retried", MODEL,
     "CappedRead::Read(err) if err.is_timeout() => CatalogError::Gateway {",
     "CappedRead::Read(err) if false && err.is_timeout() => CatalogError::Gateway {"),
    ("every failed read is retried", MODEL,
     "CappedRead::Read(err) if err.is_timeout() => CatalogError::Gateway {",
     "CappedRead::Read(err) if true || err.is_timeout() => CatalogError::Gateway {"),
    ("too large is retried", MODEL,
     "CappedRead::TooLarge => failed(larger_than(MAX_GATEWAY_CATALOG_BODY_BYTES)),",
     "CappedRead::TooLarge => CatalogError::Gateway { detail: larger_than(MAX_GATEWAY_CATALOG_BODY_BYTES), retry: Some(CatalogRetry::Unreachable) },"),
    ("bytes that are not UTF-8 read lossily", MODEL,
     "        std::str::from_utf8(&body)\n            .ok()\n            .and_then(parse_gateway_catalog)",
     "        Some(String::from_utf8_lossy(&body).into_owned())\n            .as_deref()\n            .and_then(parse_gateway_catalog)"),
    ("the deadline is the constant, not the catalog's", MODEL,
     "                .timeout(self.timeout)", "                .timeout(GATEWAY_CATALOG_TIMEOUT)"),
    ("llmux: past its cap is no failure", MODEL,
     "            CappedRead::TooLarge => larger_than(max_bytes),",
     "            CappedRead::TooLarge => return Ok(String::new()),"),
    ("credits: past its cap is read cut to fit", CREDITS,
     "        Err(CappedRead::TooLarge) => Answer::Failed(INVALID_JSON.to_string()),",
     "        Err(CappedRead::TooLarge) => answer(status, BALANCE_FOR_MUTANT, secret),"),
    ("credits: a read cut short is invalid JSON", CREDITS,
     "        Err(CappedRead::Read(_)) => failed(),",
     "        Err(CappedRead::Read(_)) => Answer::Failed(INVALID_JSON.to_string()),"),
]

def run(clone, target, cmd, **kw):
    env = dict(os.environ, CARGO_TARGET_DIR=target)
    return subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True, **kw)

def main():
    clone, target = sys.argv[1], sys.argv[2]
    killed = 0
    for name, path, old, new in MUTANTS:
        file = os.path.join(clone, path)
        text = open(file).read()
        assert text.count(old) == 1, f"{name}: the edit's text is not in {path} once"
        if "BALANCE_FOR_MUTANT" in new:
            new_text = text.replace(old, new) + '\nconst BALANCE_FOR_MUTANT: &str = r#"{"balance":"42.17","used":"7.83","plan":"pro"}"#;\n'
        else:
            new_text = text.replace(old, new)
        open(file, "w").write(new_text)
        try:
            build = run(clone, target, ["cargo", "test", "--locked", "--lib", "--no-run"])
            if build.returncode != 0:
                verdict = "killed (does not build)"
                killed += 1
            else:
                test = run(clone, target, ["python3", "-I", os.path.join(HERE, "bounded.py"), "300", "3072", "--",
                                           "cargo", "test", "--locked", "--lib", "--", "--test-threads=2", *TESTS])
                failed = [l.split()[1] for l in test.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if test.returncode != 0:
                    killed += 1
                    why = "killed by " + (", ".join(t.split("::")[-1] for t in failed) or test.stderr.strip().splitlines()[-1])
                    verdict = why
                else:
                    verdict = "SURVIVED"
        finally:
            run(clone, target, ["git", "checkout", "--", "src"])
        print(f"- {name} ({path}): {verdict}", flush=True)
    print(f"{killed} of {len(MUTANTS)} killed")
    return 0 if killed == len(MUTANTS) else 1

sys.exit(main())
