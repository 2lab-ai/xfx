//! `/model`'s search: how well a model's id matches what the composer holds,
//! and the order the matches are listed in.
//!
//! fx v0.0.16's `matchModelQuery` and its three ranks (`vercel-labs/fx@481104fd
//! src/core/app/model_cache_runtime.zig:349-447`, from f9ddaff8 *Make model
//! search fuzzy across provider and model names*), shared by the menu
//! ([`super::model_menu`], and `/settings`' Model row through it) and the
//! `/model <query>` column ([`super::model_column`]). `/model <word>` sent as a
//! command resolves by the router's own score, which fx did not change
//! ([`crate::provider::model::fuzzy_model_score`]).
//!
//! The query is read against the **whole** `provider/model` id and nothing
//! else a row carries, byte by byte as fx reads it: ASCII letters and digits
//! match ignoring case, and every other byte -- punctuation, and each byte of
//! a character outside ASCII -- is punctuation.

/// How well an id matches a query, worst to best (`ModelMatch`, `:349-367`).
/// Better matches list first; a query that names a model as written keeps the
/// rows it found before fuzzy matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Match {
    None,
    /// Each word's letters and digits appear in order in the id, the first of
    /// them at the start of an id word: `cs45` finds
    /// `anthropic/claude-sonnet-4.5`.
    Fuzzy,
    /// Each word appears in the id once punctuation is ignored:
    /// `anthropic sonnet` and `sonnet45` find `anthropic/claude-sonnet-4.5`.
    Words,
    /// The whole query appears in the id as typed, ignoring case.
    Substring,
}

/// What fx trims the query by and splits it into words at (`:375`, `:379`).
const BLANKS: [char; 4] = [' ', '\t', '\r', '\n'];

/// How well `id` matches `query` (`matchModelQuery`, `:371-389`). The blanks
/// round the query are not part of it, and an empty one matches everything.
/// Every word must match; a word with no letter or digit is passed over, so a
/// query of only those matches nothing unless the id holds it as typed. The
/// match is the worst of its words'.
pub(crate) fn of(id: &str, query: &str) -> Match {
    let text = query.trim_matches(BLANKS);
    let id = id.as_bytes();
    if contains_ignoring_case(id, text.as_bytes()) {
        return Match::Substring;
    }
    let mut result = Match::None;
    for word in text.split(BLANKS).map(str::as_bytes) {
        if !word.iter().any(u8::is_ascii_alphanumeric) {
            continue;
        }
        if alphanumeric_run(id, word) {
            if result == Match::None {
                result = Match::Words;
            }
        } else if word_start_subsequence(id, word) {
            result = Match::Fuzzy;
        } else {
            return Match::None;
        }
    }
    result
}

/// The `items` whose id matches `query`, better matches first and equal ones
/// in the order given (`modelMenuItemAt`, `:298-315`; `modelCompletions`,
/// `:895-913`).
pub(crate) fn ranked<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
    id: impl Fn(&T) -> &str,
    query: &str,
) -> Vec<&'a T> {
    let mut found: Vec<(Match, &'a T)> = items
        .into_iter()
        .map(|item| (of(id(item), query), item))
        .filter(|(rank, _)| *rank != Match::None)
        .collect();
    // Stable: a rank keeps the order it was given.
    found.sort_by(|(left, _), (right, _)| right.cmp(left));
    found.into_iter().map(|(_, item)| item).collect()
}

/// `text_utils.containsIgnoreCase` (`src/core/shared/text_utils.zig:5-15`).
fn contains_ignoring_case(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Whether `word`'s letters and digits appear consecutively among `id`'s,
/// ignoring case and every other byte (`containsAlphanumericRun`, `:398-413`).
fn alphanumeric_run(id: &[u8], word: &[u8]) -> bool {
    (0..id.len())
        .filter(|&start| id[start].is_ascii_alphanumeric())
        .any(|start| {
            let mut position = start;
            word.iter()
                .filter(|byte| byte.is_ascii_alphanumeric())
                .all(|byte| {
                    while position < id.len() && !id[position].is_ascii_alphanumeric() {
                        position += 1;
                    }
                    let hit = position < id.len() && id[position].eq_ignore_ascii_case(byte);
                    position += 1;
                    hit
                })
        })
}

/// Whether `word`'s letters and digits appear in order in `id`, the first of
/// them at the start of an id word (`containsWordStartSubsequence`,
/// `:415-436`): anchoring the first keeps a short word from matching letters
/// scattered across unrelated models.
fn word_start_subsequence(id: &[u8], word: &[u8]) -> bool {
    let mut letters = word.iter().filter(|byte| byte.is_ascii_alphanumeric());
    let Some(first) = letters.next() else {
        return true;
    };
    let Some(start) =
        (0..id.len()).find(|&index| word_start(id, index) && id[index].eq_ignore_ascii_case(first))
    else {
        return false;
    };
    let mut position = start + 1;
    letters.all(|byte| {
        while position < id.len() && !id[position].eq_ignore_ascii_case(byte) {
            position += 1;
        }
        let hit = position < id.len();
        position += 1;
        hit
    })
}

/// Whether an id word starts at `index` (`isIdWordStart`, `:438-447`): after
/// punctuation, and where letters turn into digits or back, so `gpt-4o` has
/// the words `gpt`, `4` and `o`. A change of case is not a word start.
fn word_start(id: &[u8], index: usize) -> bool {
    let byte = id[index];
    if !byte.is_ascii_alphanumeric() {
        return false;
    }
    let Some(previous) = index.checked_sub(1).map(|before| id[before]) else {
        return true;
    };
    !previous.is_ascii_alphanumeric() || previous.is_ascii_digit() != byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_match_the_full_id_verbatim_by_words_and_fuzzily() {
        // fx's table, rank for rank (`model queries match the full id
        // verbatim, by words, and fuzzily`, `:2060-2084@481104fd`).
        const ID: &str = "anthropic/claude-sonnet-4.5";
        for query in ["", " \t", " Claude-Sonnet ", "ANTHROPIC/claude"] {
            assert_eq!(of(ID, query), Match::Substring, "{query:?}");
        }
        for query in [
            "claude sonnet 4.5",
            "anthropic sonnet",
            "Sonnet45",
            "sonnet 4-5",
            "anthropic / sonnet",
        ] {
            assert_eq!(of(ID, query), Match::Words, "{query:?}");
        }
        assert_eq!(of(ID, "cs45"), Match::Fuzzy);
        assert_eq!(of(ID, "anthropic sonet"), Match::Fuzzy);
        assert_eq!(of("acme/gpt4omni", "omi"), Match::Fuzzy);
        assert_eq!(of("acme/gpt4omni", "mi"), Match::None);
        for query in ["opus", "sonnet opus", "nthrpc", "- /"] {
            assert_eq!(of(ID, query), Match::None, "{query:?}");
        }
        assert_eq!(of("", "a"), Match::None);
    }

    #[test]
    fn a_word_starts_after_punctuation_and_at_a_letter_digit_turn_only() {
        assert_eq!(of("acme/DeepSeekCoder", "dsc"), Match::Fuzzy);
        assert_eq!(of("acme/DeepSeekCoder", "sc"), Match::None);
        assert_eq!(of("openai/gpt-4o", "g4o"), Match::Fuzzy);
        assert_eq!(of("openai/gpt-4o", "o4"), Match::Fuzzy, "openai's o");
        assert_eq!(of("x/a1b2", "b"), Match::Substring);
        assert_eq!(of("x/a1b2", "x12"), Match::Fuzzy);
        assert_eq!(of("x/ab12", "b1"), Match::Substring);
        assert_eq!(of("x/ab12", "bx"), Match::None, "b is not a word start");
        // Only the first letter is anchored; the rest follow it in order.
        assert_eq!(of("q/ab-ac-ad", "aad"), Match::Fuzzy);
        assert_eq!(of("q/ab-ac", "aca"), Match::None);
    }

    #[test]
    fn the_worst_word_decides_and_a_word_of_punctuation_only_is_passed_over() {
        let id = "anthropic/claude-sonnet-4.5";
        assert_eq!(of(id, "sonet anthropic"), Match::Fuzzy);
        assert_eq!(of(id, "anthropic sonet"), Match::Fuzzy);
        assert_eq!(of(id, "claude - sonnet"), Match::Words);
        assert_eq!(of(id, "-"), Match::Substring, "as typed");
        assert_eq!(of(id, ". /"), Match::None, "no word left, not as typed");
        assert_eq!(of(id, "\u{e9}"), Match::None);
        assert_eq!(of("acme/caf\u{e9}-1", "caf\u{e9}"), Match::Substring);
        assert_eq!(
            of("acme/caf\u{e9}-1", "caf1"),
            Match::Words,
            "é is punctuation"
        );
    }

    #[test]
    fn better_ranks_list_first_and_a_rank_keeps_the_order_given() {
        let ids = [
            "openai/gpt-5",
            "anthropic/claude-opus-4.8",
            "acme/gpt4omni",
            "acme/gpt5-lite",
            "openai/gpt-4.5-preview",
        ];
        let list = |query: &str| ranked(ids.iter(), |id: &&str| *id, query);
        assert_eq!(
            list("gpt5"),
            [
                &"acme/gpt5-lite",
                &"openai/gpt-5",
                &"openai/gpt-4.5-preview"
            ]
        );
        assert_eq!(
            list("gpt 5"),
            [
                &"openai/gpt-5",
                &"acme/gpt5-lite",
                &"openai/gpt-4.5-preview"
            ]
        );
        assert_eq!(list("   ").len(), ids.len());
        assert!(list("mi").is_empty());
    }
}
