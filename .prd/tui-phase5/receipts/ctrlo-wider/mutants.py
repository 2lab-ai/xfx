"""CTRLO-WIDER's mutants: each rule taken out, one at a time, in a throwaway
clone, and the tests that pin it run against it.

usage: python3 -I mutants.py <clone> <out.txt> [<id>,<id>...]

<clone> is a `git clone --no-local` of the xfx repository at the head this
receipt names, never the worktree. The ids (`M12,M13`) pick the mutants to
run; without them, all. For each mutant the file is rewritten,
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
SH = "src/tui/shell.rs"
ANSWER_TEST = "tui::shell::tests::an_answers_characters_a_terminal_draws_wider_are_named_on_the_screen_and_kept_as_written"

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
    # Round 2: an answer's and a prompt's rows; the boundary cases; the review.
    (
        "M12 the presenter writes an answer's event as it came",
        SH,
        "        let event = full_transcript::shown_event(event);\n",
        "",
        [ANSWER_TEST],
    ),
    (
        "M13 Ctrl-O's Response rows are not named",
        FT,
        "        writer.present(shown_event(event.clone()));",
        "        writer.present(event.clone());",
        [ANSWER_TEST],
    ),
    (
        "M14 Ctrl-O's Prompt row is not named",
        FT,
        "                card(&drawn_wider_named(text), cols, palette),",
        "                card(text, cols, palette),",
        ["tui::shell::tests::a_prompts_characters_a_terminal_draws_wider_are_named_on_ctrl_o"],
    ),
    (
        "M15 an operational mark is not named",
        SH,
        "                self.write_prose(&full_transcript::drawn_wider_named(&text));",
        "                self.write_prose(&text);",
        [ANSWER_TEST],
    ),
    (
        "M16 a code panel's code is not named",
        FT,
        "            block.code = named(std::mem::take(&mut block.code));",
        "            block.code = std::mem::take(&mut block.code);",
        [ANSWER_TEST],
    ),
    (
        "M17 a table's cells are not named",
        FT,
        "                    *cell = named(std::mem::take(cell));",
        "                    *cell = std::mem::take(cell);",
        [ANSWER_TEST],
    ),
    (
        "M18 naming walks into an escape sequence",
        FT,
        "    let mut before: Option<char> = None;\n    let mut rest = text;\n    while !rest.is_empty() {\n        if let Some(len) = super::pacer::escape_at(rest) {",
        "    let mut before: Option<char> = None;\n    let mut rest = text;\n    while !rest.is_empty() {\n        if let Some(len) = None::<usize> {",
        ["tui::full_transcript::tests::an_escape_sequence_in_an_answers_text_is_passed_over_whole"],
    ),
    (
        "M19 U+302E-302F out of FX_COUNTED",
        WRAP,
        "(0x0302e, 0x0302f)",
        "(0x0302e, 0x0302d)",
        ["tui::wrap::tests::a_scalar_counted_two_cells_is_cut_placed_and_measured_at_two_by_every_consumer"],
    ),
    (
        "M20 the regional indicators out of FX_COUNTED",
        WRAP,
        "(0x1f1e6, 0x1f1ff)",
        "(0x1f1e6, 0x1f1e5)",
        ["tui::wrap::tests::a_scalar_counted_two_cells_is_cut_placed_and_measured_at_two_by_every_consumer",
         "tui::wrap::tests::an_odd_run_of_regional_indicators_counts_no_fewer_cells_than_fx"],
    ),
    (
        "M21 the review names no scalar of no cells to the table",
        AV,
        "    unicode_width::UnicodeWidthChar::width(character) == Some(0)\n        && !super::marks::is_combining_mark(character)",
        "    let _ = character;\n    false",
        ["tui::approval_view::tests::every_scalar_the_painter_counts_as_fx_does_is_still_named_in_any_context"],
    ),
    (
        "M22 Ctrl-O takes a named argument for one its heading shows",
        FT,
        "                    && first_line.contains(argument.value.as_str());",
        "                    && (first_line.contains(argument.value.as_str())\n                        || argument.value.contains(\"\\\\u{\"));",
        ["tui::full_transcript::tests::a_value_the_heading_had_to_spell_is_never_shown_by_it_already"],
    ),
    # CTRLO-WIDER-F.
    (
        "M23 a code panel's label is not named",
        FT,
        "            block.language = named(std::mem::take(&mut block.language));\n",
        "",
        ["tui::shell::tests::a_code_panels_label_names_what_a_terminal_draws_wider"],
    ),
]


def run(args, **kw):
    return subprocess.run(args, cwd=clone, capture_output=True, text=True, **kw)


only = sys.argv[3].split(",") if len(sys.argv) > 3 else None
lines = []
for ident, path, old, new, tests in MUTANTS:
    if only and ident.split()[0] not in only:
        continue
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
