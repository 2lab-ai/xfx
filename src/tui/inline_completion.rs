//! The rest of a name painted after the caret: a `$word` a skill's name begins
//! with, or a slash word in the middle of a draft that a command's name begins
//! with.
//!
//! Upstream's inline completion (`vercel-labs/fx@c1db9191
//! src/core/app/input_completion_runtime.zig:598-640`): the **first** name the
//! word begins -- the skills in discovery order, case aside
//! (`firstSkillNameCompletion`, `src/core/skills/skill_runtime.zig:1397-1408`),
//! the commands in registry order, a command's name before its aliases
//! (`firstSlashCompletion`, `src/core/slash_commands/command_specs.zig:490-506`)
//! -- with what the word has not typed yet painted after it in the dim grey and
//! closed by a reset (`appendInlineCompletionSuffix`,
//! `src/ui/footer/input_presentation.zig:955-973`). Right and Tab take it;
//! Enter sends the draft as it is.
//!
//! The words are upstream's (`findInlineSkillQuery`,
//! `findInlineSlashQuery`, `src/core/input/picker_state.zig:524-553`), and
//! both need the caret at the end of the draft:
//!
//! * a **`$word`** is the run back to the nearest `$` with no blank in it, the
//!   `$` not inside an `@` path;
//! * a **slash word** is the last word, starting with `/` and at least one
//!   letter more, with something other than blanks before it. A draft that
//!   **is** a slash word is the slash popup's ([`super::picker`]), never this.
//!
//! What owns the surface instead -- a question, a catalog, the skills menu
//! while it shows -- and what a dismissal suppresses are the shell's, which
//! holds the rest of the state (`inlineCompletionSurfaceAvailable`,
//! `:703-727`).

use crate::interactive::SlashSpec;
use crate::skills::Skill;

use super::wrap;

/// A byte that ends a word for both queries (`isFilePickerTerminator`,
/// `picker_state.zig:508-510`).
fn terminator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// The `$word` the caret is at the end of: where its `$` is
/// (`findInlineSkillQuery`, `picker_state.zig:524-541`).
pub(crate) fn skill_word(text: &str, caret: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if caret != bytes.len() {
        return None;
    }
    let mut index = caret;
    while index > 0 {
        let byte = bytes[index - 1];
        if terminator(byte) {
            return None;
        }
        index -= 1;
        if byte != b'$' {
            continue;
        }
        if super::file_picker::contains_position(bytes, index) {
            return None;
        }
        return Some(index);
    }
    None
}

/// The slash word the caret is at the end of, with its slash
/// (`findInlineSlashQuery`, `picker_state.zig:543-553`).
pub(crate) fn slash_word(text: &str, caret: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if caret != bytes.len() {
        return None;
    }
    let mut start = caret;
    while start > 0 && !terminator(bytes[start - 1]) {
        start -= 1;
    }
    if start == 0 || start + 1 >= caret || bytes[start] != b'/' {
        return None;
    }
    if text[..start]
        .trim_matches([' ', '\t', '\r', '\n'])
        .is_empty()
    {
        return None;
    }
    Some(&text[start..caret])
}

/// The first skill whose name `query` begins, ASCII case aside, and the rest
/// of its name (`firstSkillNameCompletion`).
pub(crate) fn first_skill<'a>(skills: &'a [Skill], query: &str) -> Option<(&'a Skill, &'a str)> {
    if query.is_empty() {
        return None;
    }
    skills
        .iter()
        .find(|skill| {
            let name = skill.name.as_bytes();
            name.len() > query.len()
                && name[..query.len()].eq_ignore_ascii_case(query.as_bytes())
                && skill.name.is_char_boundary(query.len())
        })
        .map(|skill| (skill, &skill.name[query.len()..]))
}

/// The first command whose name -- or, after it, whose alias -- `prefix`
/// begins and is shorter than (`firstSlashCompletion`): the name whole.
pub(crate) fn first_command(registry: &'static [SlashSpec], prefix: &str) -> Option<&'static str> {
    if !prefix.starts_with('/') {
        return None;
    }
    for spec in registry {
        if spec.name.starts_with(prefix) && spec.name.len() > prefix.len() {
            return Some(spec.name);
        }
        if let Some(alias) = spec
            .aliases
            .iter()
            .find(|alias| alias.starts_with(prefix) && alias.len() > prefix.len())
        {
            return Some(alias);
        }
    }
    None
}

/// Whether the suffix's first character fits in the `available` cells after
/// the caret (`inlineSuffixCanRender`, `input_completion_runtime.zig:675-701`):
/// a ghost that would not show is not there to be taken either.
pub(crate) fn can_render(suffix: &str, available: u16) -> bool {
    wrap::first_cluster(suffix).is_some_and(|first| {
        let cells = wrap::cluster_width(first);
        cells > 0 && cells <= available
    })
}

/// `suffix` cut to the clusters that fit in `room` cells.
pub(crate) fn clipped(suffix: &str, room: u16) -> &str {
    let mut used = 0u16;
    let mut end = 0;
    let mut rest = suffix;
    while let Some(cluster) = wrap::first_cluster(rest) {
        let cells = wrap::cluster_width(cluster);
        if used + cells > room {
            break;
        }
        used += cells;
        end += cluster.len();
        rest = &rest[cluster.len()..];
    }
    &suffix[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::SLASH_REGISTRY;
    use crate::skills::Source;
    use std::path::PathBuf;

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: String::new(),
            path: PathBuf::from(format!("/tmp/{name}")),
            source: Source::Managed,
        }
    }

    #[test]
    fn a_dollar_word_is_the_run_back_to_its_dollar_with_the_caret_at_the_end() {
        // `explain $man` (`input_completion_runtime.zig` and
        // `app_input_runtime.zig`'s cases).
        assert_eq!(skill_word("explain $man", 12), Some(8));
        assert_eq!(skill_word("$", 1), Some(0));
        // The caret anywhere else, a blank on the way back, or no `$`.
        assert_eq!(skill_word("explain $man", 11), None);
        assert_eq!(skill_word("$man ", 5), None);
        assert_eq!(skill_word("man", 3), None);
        // The nearest `$`, and a `$` inside an `@` path is a path.
        assert_eq!(skill_word("$a$b", 4), Some(2));
        assert_eq!(skill_word("@dir/$x", 7), None);
    }

    #[test]
    fn a_slash_word_needs_words_before_it_and_a_letter_after_its_slash() {
        assert_eq!(slash_word("explain /he", 11), Some("/he"));
        assert_eq!(slash_word("x /he", 5), Some("/he"));
        // The whole draft is the popup's, not this.
        assert_eq!(slash_word("/he", 3), None);
        assert_eq!(slash_word("   /he", 6), None);
        // A bare slash, the caret short of the end, or a word without one.
        assert_eq!(slash_word("explain /", 9), None);
        assert_eq!(slash_word("explain /he", 10), None);
        assert_eq!(slash_word("explain he", 10), None);
    }

    #[test]
    fn the_first_skill_whose_name_the_word_begins_case_aside() {
        let skills = [skill("managed-menu"), skill("man"), skill("Manual")];
        assert_eq!(
            first_skill(&skills, "man").map(|(skill, rest)| (skill.name.as_str(), rest)),
            Some(("managed-menu", "aged-menu"))
        );
        // A name no longer than the word is not completed.
        assert_eq!(
            first_skill(&skills, "MANU").map(|(skill, rest)| (skill.name.as_str(), rest)),
            Some(("Manual", "al"))
        );
        assert!(first_skill(&skills, "manual").is_none());
        assert!(first_skill(&skills, "").is_none());
    }

    #[test]
    fn the_first_command_in_registry_order_its_name_before_its_alias() {
        assert_eq!(first_command(SLASH_REGISTRY, "/he"), Some("/help"));
        // `/quit`'s alias `/exit`, found under its command's row.
        assert_eq!(first_command(SLASH_REGISTRY, "/exi"), Some("/exit"));
        // Case-sensitive, and nothing longer than an exact name.
        assert_eq!(first_command(SLASH_REGISTRY, "/HE"), None);
        assert_eq!(first_command(SLASH_REGISTRY, "/help"), None);
        assert_eq!(first_command(SLASH_REGISTRY, "he"), None);
    }

    #[test]
    fn a_suffix_shows_only_when_its_first_character_fits_and_is_cut_to_the_room() {
        // `x $man` on an 8-column screen leaves no cell after `┃ x $man`; on a
        // 9-column one it leaves one (`inline skill completion stays inactive
        // when its suffix cannot render`).
        assert!(!can_render("aged-menu", 0));
        assert!(can_render("aged-menu", 1));
        assert!(!can_render("", 5));
        assert_eq!(clipped("aged-menu", 4), "aged");
        assert_eq!(clipped("aged-menu", 40), "aged-menu");
        assert_eq!(clipped("\u{4e2d}a", 1), "");
    }
}
