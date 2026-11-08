"""Append the train head's join tests (the shell.rs section from its
'Train 6 joins' banner to the module's end, and toolgroup.rs's one test) to a
unit's clone, unchanged. usage: apply_join.py <head-worktree> <clone>"""
import sys
head, clone = sys.argv[1:3]
def section(path, start):
    s = open(f"{head}/{path}", encoding="utf-8").read()
    i = s.index(start)
    # back to the start of the line
    i = s.rindex("\n", 0, i) + 1
    body = s[i:]
    assert body.endswith("}\n")
    return body[:-2]  # without the module's closing brace
for path, start in (("src/tui/shell.rs", "    // -----------------------------------------------------------------------\n    // Train 6 joins"),
                    ("src/tui/toolgroup.rs", "    #[test]\n    fn the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width")):
    body = section(path, start)
    t = open(f"{clone}/{path}", encoding="utf-8").read()
    assert t.endswith("    }\n}\n")
    open(f"{clone}/{path}", "w", encoding="utf-8").write(t[:-2] + "\n" + body + "}\n")
    print(path, "appended", body.count("\n"), "lines")
