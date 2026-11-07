"""CTRLO-WIDER's mutants: each rule taken out, one at a time, in a throwaway
clone, and the tests that pin it run against it.

usage: python3 -I mutants.py <clone> <out.txt>

<clone> is a `git clone --no-local` of the xfx repository at the head this
receipt names, never the worktree. For each mutant the file is rewritten,
the named tests run (`cargo test --locked --lib -- <tests>`, CARGO_TARGET_DIR
from the environment), the file restored from git, and one line written:
`<id> killed|SURVIVED <failing tests>`.
"""
import os
import subprocess
import sys

clone, out_path = sys.argv[1], sys.argv[2]
WRAP = "src/tui/wrap.rs"
FT = "src/tui/full_transcript.rs"
AV = "src/tui/approval_view.rs"

MUTANTS = [
    (
        "M1 fx_counted is never true",
        WRAP,
        "fn fx_counted(character: char) -> bool {\n    let code = u32::from(character);",
        "fn fx_counted(character: char) -> bool {\n    let _ = character;\n    return false;\n    #[allow(unreachable_code)]\n    let code = u32::from(character);",
        ["tui::wrap::tests::the_scalars_a_terminal_drew_wider_take_the_cells_fx_counts",
         "tui::wrap::tests::a_cluster_with_one_of_them_takes_the_cells_fx_counts_and_a_flag_stays_two"],
    ),
    (
        "M2 the last FX_COUNTED range dropped",
        WRAP,
        "(0x1f1e6, 0x1f1ff), (0xe0000, 0xe0000), (0xe0080, 0xe00ff), (0xe01f0, 0xe0fff),\n];",
        "(0x1f1e6, 0x1f1ff), (0xe0000, 0xe0000), (0xe0080, 0xe00ff), (0xe01f0, 0xe01f0),\n];",
        ["tui::wrap::tests::the_scalars_a_terminal_drew_wider_take_the_cells_fx_counts",
         "tui::wrap::tests::no_other_scalar_is_counted_otherwise"],
    ),
    (
        "M3 upstream_cells a code point at a time (an RGI flag four cells)",
        WRAP,
        "        let unit = crate::markdown::display_width::display_unit_at(bytes, index);",
        "        let mut unit = crate::markdown::display_width::display_unit_at(bytes, index);\n        unit = crate::markdown::display_width::DisplayUnit { byte_len: cluster[index..].chars().next().map_or(1, char::len_utf8), cell_width: crate::markdown::display_width::visible_width(cluster[index..].chars().next().map_or(\"\", |c| &cluster[index..index + c.len_utf8()]).as_bytes()) };",
        ["tui::wrap::tests::a_cluster_with_one_of_them_takes_the_cells_fx_counts_and_a_flag_stays_two"],
    ),
    (
        "M4 upstream_cells without the width table's floor",
        WRAP,
        "        cells += unit.cell_width.max(table);",
        "        let _ = table;\n        cells += unit.cell_width;",
        ["tui::wrap::tests::a_filler_cluster_never_counts_fewer_cells_than_the_width_table_or_a_terminal_draws"],
    ),
    (
        "M5 the review names only the table (a regional indicator not)",
        AV,
        "    super::drawn_wider::drawn_wider(character) || regional_indicator(character)\n}",
        "    super::drawn_wider::drawn_wider(character)\n}",
        ["tui::approval_view::tests::a_command_names_every_table_character_itself",
         "tui::approval_view::tests::the_measured_corpus_is_laid_out_at_least_as_wide_as_both_terminals_draw_it",
         "tui::approval_view::tests::every_spelling_keeps_its_postcondition_over_generated_text"],
    ),
    (
        "M6 any pair of regional indicators is a flag",
        AV,
        "        (Some(first), Some(second)) if regional_indicator(first) && regional_indicator(second) => {\n            crate::markdown::display_width::rgi_sequence_len(line.as_bytes(), index)\n                == first.len_utf8() + second.len_utf8()\n        }",
        "        (Some(first), Some(second)) if regional_indicator(first) && regional_indicator(second) => {\n            true\n        }",
        ["tui::approval_view::tests::the_measured_corpus_is_laid_out_at_least_as_wide_as_both_terminals_draw_it",
         "tui::approval_view::tests::every_spelling_keeps_its_postcondition_over_generated_text"],
    ),
    (
        "M7 Ctrl-O names nothing the table holds",
        FT,
        "    if text.is_ascii() || !text.chars().any(super::drawn_wider::drawn_wider) {",
        "    if true || text.is_ascii() || !text.chars().any(super::drawn_wider::drawn_wider) {",
        ["tui::full_transcript::tests::a_character_a_terminal_draws_wider_than_it_is_counted_is_named_in_a_files_line"],
    ),
    (
        "M8 Ctrl-O names an emoji base with its selector too",
        FT,
        "        if super::drawn_wider::drawn_wider(character) && !agreed && !undrawn {",
        "        if super::drawn_wider::drawn_wider(character) && !undrawn {",
        ["tui::full_transcript::tests::a_character_a_terminal_draws_wider_than_it_is_counted_is_named_in_a_files_line"],
    ),
    (
        "M9 Ctrl-O names a selector right after a tab",
        FT,
        "        if super::drawn_wider::drawn_wider(character) && !agreed && !undrawn {",
        "        if super::drawn_wider::drawn_wider(character) && !agreed {",
        ["tui::full_transcript::tests::a_character_a_terminal_draws_wider_than_it_is_counted_is_named_in_a_files_line",
         "tui::full_transcript::tests::a_mark_right_after_a_tab_is_not_drawn_as_fxs_screen_does_not_draw_it"],
    ),
    (
        "M10 a call's detail is not named (diff rows only)",
        FT,
        "        out.push_str(&drawn_wider_named(&spelled_line));",
        "        out.push_str(&spelled_line);",
        ["tui::full_transcript::tests::a_character_a_terminal_draws_wider_than_it_is_counted_is_named_in_a_files_line"],
    ),
    (
        "M11 a diff row is not named (a call's detail only)",
        FT,
        "    drawn_wider_named(&out).into_owned()\n}",
        "    out\n}",
        ["tui::full_transcript::tests::a_character_a_terminal_draws_wider_than_it_is_counted_is_named_in_a_files_line"],
    ),
]


def run(args, **kw):
    return subprocess.run(args, cwd=clone, capture_output=True, text=True, **kw)


lines = []
for ident, path, old, new, tests in MUTANTS:
    full = os.path.join(clone, path)
    text = open(full, encoding="utf-8").read()
    if text.count(old) != 1:
        lines.append(f"{ident}: NOT APPLIED ({text.count(old)} matches)")
        continue
    open(full, "w", encoding="utf-8").write(text.replace(old, new))
    try:
        result = run(["cargo", "test", "--locked", "--lib", "--", *tests], timeout=1800)
    finally:
        run(["git", "checkout", "--", path])
    failed = [t for t in tests if f"{t} ... FAILED" in result.stdout]
    if "error[E" in result.stderr or "could not compile" in result.stderr:
        lines.append(f"{ident}: DID NOT BUILD")
    elif failed:
        lines.append(f"{ident}: killed by {', '.join(t.rsplit('::', 1)[1] for t in failed)}")
    else:
        lines.append(f"{ident}: SURVIVED")
    print(lines[-1], flush=True)
with open(out_path, "w") as out:
    out.write("# CTRLO-WIDER mutants, each on a throwaway clone at the head (python3 -I mutants.py)\n")
    out.write("\n".join(lines) + "\n")
