"""MCP-Me (`.prd/tui-phase5/receipts/mcp-menu/README.md`): each mutant removes one guard (or one pinned term) in the
throwaway clone, runs the tests that should catch it, records whether they
failed, and restores the files. Never run on the worktree."""
import os
import subprocess
import sys

root = sys.argv[1]
assert "/scratchpad/" in root, root
MENU = "src/tui/mcp_menu.rs"
SHELL = "src/tui/shell.rs"
LOOP = "src/tui/event_loop.rs"
RT = "src/mcp/runtime.rs"
CMD = "src/mcp/command.rs"
T_MENU = "tui::mcp_menu::tests::"
T_SHELL = "tui::shell::tests::"

PROC = "src/process.rs"
TRANSPORT = "src/mcp/transport.rs"
M = [
    ("spawn-unlocked", [
        (PROC, "    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);\n", "    let _ = &SPAWNING;\n"),
    ], ["process::tests::children_started_at_once_never_hold_up_one_anothers_start"], True),
    ("mcp-start-bypasses", [
        (TRANSPORT, "crate::process::spawn(&mut builder).map_err(", "builder.spawn().map_err("),
    ], ["process::tests::no_child_process_is_made_any_other_way"], True),
]

only = set(sys.argv[2:])
for name, edits, tests, exact in M:
    if only and name not in only:
        continue
    saved = {}
    try:
        for path, old, new in edits:
            f = os.path.join(root, path)
            src = saved.get(f) or open(f).read()
            saved.setdefault(f, src)
            cur = open(f).read()
            assert cur.count(old) == 1, (name, path, cur.count(old))
            open(f, "w").write(cur.replace(old, new))
        cmd = ["cargo", "test", "--locked", "--lib", "--"] + (["--exact"] if exact else []) + tests
        p = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=1200)
        out = p.stdout + p.stderr
        compiled = "error[" not in out and "could not compile" not in out
        failed = sorted({line.split()[1] for line in out.splitlines()
                         if line.startswith("test ") and line.endswith("... FAILED")})
        verdict = "CAUGHT" if (compiled and p.returncode != 0 and failed) else (
            "COMPILE-ERROR" if not compiled else "MISSED")
        print(f"{name}: {verdict} exit={p.returncode} failed={failed}", flush=True)
        if not compiled:
            print(out[-3000:], flush=True)
    finally:
        for f, src in saved.items():
            open(f, "w").write(src)
