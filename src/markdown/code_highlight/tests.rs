use std::borrow::Cow;

use super::*;
use crate::markdown::theme::FX_DARK;

/// `code_highlight.zig:449-456`: only CSI sequences, and a bare escape is
/// kept.
fn ansi_sequence_end(text: &[u8], start: usize) -> usize {
    if start + 2 > text.len() || text[start] != 0x1b || text[start + 1] != b'[' {
        return start;
    }
    let mut index = start + 2;
    while index < text.len() {
        if (b'@'..=b'~').contains(&text[index]) {
            return index + 1;
        }
        index += 1;
    }
    start
}

fn strip_ansi(text: &str) -> String {
    let text = text.as_bytes();
    let mut plain = Vec::new();
    let mut index = 0;
    while index < text.len() {
        if text[index] == 0x1b {
            let end = ansi_sequence_end(text, index);
            if end > index {
                index = end;
                continue;
            }
        }
        plain.push(text[index]);
        index += 1;
    }
    String::from_utf8(plain).expect("stripping escapes keeps UTF-8")
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

fn profile(label: &str) -> &'static Profile {
    languages::resolve(label).expect("registered profile")
}

fn dark(source: &str, label: &str) -> String {
    highlight(source, profile(label), Variant::Dark, None, &FX_DARK)
}

#[test]
fn supported_source_gains_balanced_colors_without_changing_code_bytes() {
    let source = "const value = \"const\"; // return\n";
    let styled = dark(source, "zig");

    assert_eq!(strip_ansi(&styled), source);
    assert!(styled.contains("\x1b[38;5;"));
    assert_eq!(count(&styled, "\x1b[38;5;"), count(&styled, "\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252mconst"));
    assert_eq!(count(&styled, "\x1b[38;5;252mconst"), 1);
    assert!(styled.contains("\x1b[38;5;250m\"const\"\x1b[39m"));
    assert!(!styled.contains("\x1b[38;5;252mreturn"));
}

#[test]
fn light_theme_uses_a_readable_syntax_palette_without_changing_code_bytes() {
    let source = "const value = \"ready\"; // comment\n";
    let styled = highlight(source, profile("zig"), Variant::Light, None, &FX_DARK);

    assert_eq!(strip_ansi(&styled), source);
    assert!(styled.contains("\x1b[38;5;238mconst\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;241m\"ready\"\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;243m// comment\x1b[39m"));
    assert_eq!(count(&styled, "\x1b[38;5;"), count(&styled, "\x1b[39m"));
}

#[test]
fn every_registered_profile_highlights_representative_source() {
    let cases = [
        ("zig", "pub fn main() void { return; }"),
        ("ts", "const ready = true;"),
        ("json", "{\"ready\": true}"),
        ("sh", "if true; then echo \"ready\"; fi"),
        ("python", "def ready(): return True"),
        ("yaml", "ready: true # comment"),
        ("toml", "ready = true # comment"),
        ("sql", "SELECT id FROM users"),
        ("dockerfile", "FROM alpine:3.20"),
        ("rust", "fn main() { let ready = true; }"),
        ("go", "package main\nfunc main() {}"),
        ("c", "int main(void) { return 0; }"),
        ("cpp", "class Ready { public: bool value = true; };"),
        ("csharp", "public class Ready { }"),
        ("java", "public class Ready { }"),
        ("kotlin", "fun ready(): Boolean = true"),
        ("php", "<?php function ready() { return true; }"),
        ("ruby", "def ready\n  true\nend"),
        ("swift", "func ready() -> Bool { true }"),
        ("powershell", "Function Ready { return $true }"),
        ("lua", "local ready = true"),
        ("html", "<main class=\"ready\"></main>"),
        ("xml", "<?xml version=\"1.0\"?>"),
        ("css", ".ready { color: red; }"),
        ("hcl", "resource \"ready\" \"main\" {}"),
    ];

    for (label, source) in cases {
        let styled = dark(source, label);
        assert_eq!(strip_ansi(&styled), source, "{label}");
        assert!(styled.contains("\x1b[38;5;"), "{label}");
    }
}

#[test]
fn profiles_use_configured_block_comments_and_case_insensitive_keywords() {
    let source = "/* comment */\nSELECT id FROM users\n<!-- note -->";
    let css = dark(&source[..13], "css");
    let sql = dark(&source[14..34], "sql");
    let html = dark(&source[35..], "html");

    assert!(css.contains("\x1b[38;5;245m/* comment */\x1b[39m"));
    assert!(sql.contains("\x1b[38;5;252mSELECT\x1b[39m"));
    assert!(html.contains("\x1b[38;5;245m<!-- note -->\x1b[39m"));
}

#[test]
fn themed_attribute_slots_close_fully_without_bleeding_into_later_text() {
    let mut custom = FX_DARK;
    custom.syntax.comment_style = Cow::Borrowed("\x1b[3;38;2;106;153;85m");
    custom.syntax.keyword_style = Cow::Borrowed("\x1b[1;38;2;130;210;206m");

    let styled = highlight(
        "const x = 1; // note\n",
        profile("zig"),
        Variant::Dark,
        None,
        &custom,
    );

    // Italic comment and bold keyword each close with their attributes reset.
    assert!(styled.contains("\x1b[3;38;2;106;153;85m// note\x1b[39m\x1b[23m"));
    assert!(styled.contains("\x1b[1;38;2;130;210;206mconst\x1b[39m\x1b[22m"));
    // Nothing stays bold or italic past the final close.
    assert!(!styled.ends_with("\x1b[3;38;2;106;153;85m"));
}

#[test]
fn base_style_wraps_the_span_and_restores_after_each_token() {
    let styled = highlight(
        "echo 'hi there' 42",
        profile("sh"),
        Variant::Dark,
        Some("<base>"),
        &FX_DARK,
    );

    // The span opens with the base, and every token close re-establishes it.
    assert!(styled.starts_with("<base>\x1b[38;5;252mecho\x1b[39m<base> "));
    assert!(styled.contains("\x1b[38;5;250m'hi there'\x1b[39m<base>"));
    // Bare number arguments stay plain in shell.
    assert!(styled.ends_with("<base> 42"));
}

#[test]
fn a_theme_with_syntax_disabled_passes_the_source_through() {
    let mut no_syntax = FX_DARK;
    no_syntax.syntax.enabled = false;

    let plain = highlight(
        "echo 'hi there' 42",
        profile("sh"),
        Variant::Dark,
        None,
        &no_syntax,
    );
    assert_eq!(plain, "echo 'hi there' 42");

    let with_base = highlight(
        "echo 'hi there' 42",
        profile("sh"),
        Variant::Dark,
        Some("<base>"),
        &no_syntax,
    );
    assert_eq!(with_base, "<base>echo 'hi there' 42");
}

#[test]
fn shell_operators_and_variables_take_the_keyword_color() {
    let styled = dark(
        "cd /tmp && echo $HOME | head -2 > out; echo $? # done",
        "sh",
    );

    assert!(styled.contains("\x1b[38;5;252m&&\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m|\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m>\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m;\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m$HOME\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m$?\x1b[39m"));
    // Comments keep their colour, flags take the number colour as a unit,
    // and a $ inside quotes stays string.
    assert!(styled.contains("\x1b[38;5;245m# done\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;250m-2\x1b[39m"));

    let quoted = dark("echo '$HOME'", "sh");
    assert!(quoted.contains("\x1b[38;5;250m'$HOME'\x1b[39m"));

    // Other languages do not pick up shell operators.
    assert_eq!(dark("a < b", "zig"), "a < b");
}

#[test]
fn digit_runs_glued_to_words_by_a_dash_stay_plain() {
    let styled = dark("cd build-20260918 && head -80 2>/dev/null", "sh");

    // The date suffix in the path is a name segment and keeps the plain
    // text, as does the literal-looking "null" in /dev/null.
    assert!(styled.contains("build-20260918"));
    assert!(styled.contains("/dev/null"));
    // The numeric flag colours as a unit, and the redirect fd still colours.
    assert!(styled.contains("\x1b[38;5;250m-80\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;250m2\x1b[39m"));
}

#[test]
fn dash_flags_color_as_units_only_at_word_boundaries() {
    let styled = dark("tail -8 --json && cat - < in", "sh");

    assert!(styled.contains("\x1b[38;5;250m-8\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;250m--json\x1b[39m"));
    // A lone dash (stdin marker) stays plain between the verb and the
    // redirect, and the redirect target is an argument, not a keyword.
    assert!(styled.contains("\x1b[38;5;252mcat\x1b[39m - \x1b[38;5;252m<\x1b[39m in"));

    // Flags after operators still count as boundaries.
    let after_pipe = dark("echo x | head -1", "sh");
    assert!(after_pipe.contains("\x1b[38;5;250m-1\x1b[39m"));

    // Other languages keep minus signs plain.
    assert_eq!(dark("a - b", "zig"), "a - b");
}

#[test]
fn command_position_colors_any_command_word_and_only_command_words() {
    let styled = dark("gh run list | xargs echo > out.txt", "sh");

    // Unknown binaries colour in command position, matching the bash
    // grammar's variable.function; a builtin used as an argument stays
    // plain, and the redirect target is a path, not a command.
    assert!(styled.contains("\x1b[38;5;252mgh\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252mxargs\x1b[39m"));
    // echo is an argument here and stays plain; the redirect target too.
    assert!(styled.contains(" echo \x1b[38;5;252m>\x1b[39m out.txt"));

    // Control keywords hand command position to the command they govern.
    let chain = dark("if cd /x; then echo hi; fi", "sh");
    for word in ["if", "cd", "then", "echo", "fi"] {
        let wrapped = format!("\x1b[38;5;252m{word}\x1b[39m");
        assert!(chain.contains(&wrapped), "{word}");
    }
}

#[test]
fn bare_number_arguments_stay_plain_but_redirect_fds_color() {
    let styled = dark("sleep 5; exit 7 2>&1", "sh");

    assert!(styled.contains(" 5"));
    assert!(styled.contains(" 7 "));
    assert!(styled.contains("\x1b[38;5;250m2\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;250m1\x1b[39m"));
}

#[test]
fn braced_variables_tildes_globs_and_substitution_parse_like_the_grammar() {
    let styled = dark("cp ${SRC}/*.log ~/out && echo $(date +%F)", "sh");

    assert!(styled.contains("\x1b[38;5;252m${SRC}\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m*\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m~\x1b[39m/out"));
    assert!(styled.contains("\x1b[38;5;252m$(\x1b[39m"));
    // The substitution contents open in command position.
    assert!(styled.contains("\x1b[38;5;252mdate\x1b[39m"));

    // Backtick contents parse as code rather than one flat string.
    let ticks = dark("echo `uname -s`", "sh");
    assert!(ticks.contains("`\x1b[38;5;252muname\x1b[39m"));
    assert!(ticks.contains("\x1b[38;5;250m-s\x1b[39m"));
}

#[test]
fn double_quotes_interpolate_variables_inside_the_string_color() {
    let styled = dark("echo \"hi $USER from ${HOME}\"", "sh");

    assert!(styled.contains("\x1b[38;5;250m\"hi \x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m$USER\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;252m${HOME}\x1b[39m"));
    // Single quotes do not interpolate.
    let single = dark("echo '$USER'", "sh");
    assert!(single.contains("\x1b[38;5;250m'$USER'\x1b[39m"));
}

#[test]
fn diff_lines_paint_with_the_caller_s_marker_colors() {
    let patch = "diff --git a/f b/f\nindex 111..222 100644\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-old line\n+new line\n context";
    let styled = highlight_diff(patch, Variant::Dark, "<added>", "<removed>", &FX_DARK);

    assert!(styled.contains("<added>+new line"));
    assert!(styled.contains("<removed>-old line"));
    assert!(styled.contains("\x1b[38;5;252m@@ -1,2 +1,2 @@\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;245m--- a/f\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;245mdiff --git a/f b/f\x1b[39m"));
    assert!(styled.contains("\n context"));

    let mut no_syntax = FX_DARK;
    no_syntax.syntax.enabled = false;
    let plain = highlight_diff(patch, Variant::Dark, "<added>", "<removed>", &no_syntax);
    assert_eq!(plain, patch);
}

#[test]
fn text_blocks_stay_byte_identical_and_markdown_colors_inline_code() {
    let text = dark("plain prose with 42 numbers and # no comment", "text");
    assert_eq!(text, "plain prose with 42 numbers and # no comment");

    let md = dark("run `fx upgrade` to update", "md");
    assert!(md.contains("\x1b[38;5;250m`fx upgrade`\x1b[39m"));
}

#[test]
fn split_slots_let_themes_color_commands_variables_and_operators_apart() {
    let mut themed = FX_DARK;
    themed.syntax.function_style = Cow::Borrowed("\x1b[38;5;201m");
    themed.syntax.variable_style = Cow::Borrowed("\x1b[38;5;202m");
    themed.syntax.operator_style = Cow::Borrowed("\x1b[38;5;203m");
    themed.syntax.keyword_style = Cow::Borrowed("\x1b[38;5;204m");

    let styled = highlight(
        "while true; do echo $HOME | head -2; done",
        profile("sh"),
        Variant::Dark,
        None,
        &themed,
    );

    // Control keywords keep the keyword colour; command words take the
    // function colour; variables and operators take their own.
    assert!(styled.contains("\x1b[38;5;204mwhile\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;201mtrue\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;201mecho\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;202m$HOME\x1b[39m"));
    assert!(styled.contains("\x1b[38;5;203m|\x1b[39m"));
}
