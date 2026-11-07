#!/usr/bin/env python3
"""The MCP-E x TC-NOTATION join test's mutants: each takes one link out of
the path a 2026 server's version reaches the screen by, in a throwaway clone,
and runs the join test against it.

    python3 -I -B mutants.py <clone of the train head>

Each mutant is one exact replacement in one file, put back after its run. A
mutant is caught when the test fails.
"""
import subprocess
import sys

TEST = "a_2026_servers_version_and_protocol_rows_are_spelt_as_upstream_spells_its_greys"
MUTANTS = [
    ("A", "the notice grey spelt direct (the dark 250 as 38;2)", "src/tui/theme.rs",
     "    pub(crate) fn notice(&self) -> &'static str {\n        self.paint(NOTICE)\n    }",
     "    pub(crate) fn notice(&self) -> &'static str {\n        \"\\u{1b}[38;2;188;188;188m\"\n    }"),
    ("B", "the server screen's fact label spelt direct (the dim grey as 38;2)", "src/tui/mcp_menu.rs",
     "    let mut out = String::from(palette.dim());",
     "    let mut out = String::from(\"\\u{1b}[38;2;158;158;158m\");"),
    ("C", "the menu's row drops the era's version", "src/tui/mcp_menu.rs",
     "        version: view.protocol_version.clone(),",
     "        version: None,"),
    ("D", "`/mcp show` says no version", "src/mcp/command.rs",
     "        out.push_str(&format!(\"  version    {}\\n\", safe(version)));",
     "        let _ = version;"),
    ("E", "a theme switch leaves a notice row in its first palette", "src/tui/theme.rs",
     "            DocumentRow::Plain => return self.document_retint(colour),",
     "            DocumentRow::Plain => return None,"),
]


def run(clone):
    done = subprocess.run(["cargo", "test", "--locked", "--lib", TEST], cwd=clone, capture_output=True, text=True)
    text = done.stdout + done.stderr
    result = [line for line in text.splitlines() if line.startswith("test result") or " ... FAILED" in line]
    panic = [line for line in text.splitlines() if "panicked at" in line or line.startswith("assertion") or "never written" in line]
    return done.returncode, result, panic[:2]


def main():
    clone = sys.argv[1]
    code, result, _ = run(clone)
    print(f"unmutated: exit {code}; " + "; ".join(result))
    caught = 0
    for name, what, path, old, new in MUTANTS:
        full = f"{clone}/{path}"
        original = open(full, encoding="utf-8").read()
        assert original.count(old) == 1, name
        open(full, "w", encoding="utf-8").write(original.replace(old, new))
        try:
            code, result, panic = run(clone)
        finally:
            open(full, "w", encoding="utf-8").write(original)
        failed = any(" ... FAILED" in line for line in result)
        caught += failed
        print(f"{name} ({what}; {path}): {'CAUGHT' if failed else 'SURVIVED'}; " + "; ".join(result))
        for line in panic:
            print(f"    {line.strip()[:200]}")
    print(f"{caught} of {len(MUTANTS)} caught")
    sys.exit(0 if caught == len(MUTANTS) else 1)


main()
