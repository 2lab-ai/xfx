//! AT-RANK's search, kept as it was at `4a5dc3c7` (`src/tui/file_index.rs`,
//! its ranking section, lines 527-1152) for the equivalence cases only: every
//! path scored in full, then the best [`MOST_RESULTS`]. AT-RANK-PERF's search
//! must give the same rows, kinds and bold letters. Nothing here is shared
//! with the search it checks -- not the constants, not the dynamic program --
//! so a change there cannot move the reference with it. The lines are
//! AT-RANK's unchanged; only the imports above them are new, and the
//! [`Candidate`] and [`Found`] types are the index's own.

use std::cmp::Ordering;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::{Candidate, Found, MOST_RESULTS};

/// What a match is ranked on (`MatchScore`, `file_index.zig:1254-1265@481104fd`),
/// in the order [`score_order`] compares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Score {
    /// The query is the path, the name, or the name up to a dot (`exactFit`).
    exact: bool,
    /// The name tier: the placement is inside the name or runs from the
    /// folder holding it into the name. Off for the last search over the
    /// whole path, which ranks below every name match even where that
    /// placement happens to end in the name (`ab/x/cd.ts` for `abcd`).
    name: bool,
    /// The placement's score, [`NAME_BONUS`] in it for one inside the name.
    quality: i32,
    /// Its first scalar, counted from where its search began.
    first: usize,
}

/// Whether `left` is the better match (`scoreBetter`, `file_index.zig:1884-1890`).
fn score_order(left: &Score, right: &Score) -> Ordering {
    right
        .exact
        .cmp(&left.exact)
        .then(right.name.cmp(&left.name))
        .then(right.quality.cmp(&left.quality))
        .then(left.first.cmp(&right.first))
}

/// The whole order: the score, then the shorter path, then bytes
/// (`rankedCandidateBetter`, `file_index.zig:1892-1899`; `NameQuery.better`,
/// `:1302-1309`, which skips the first two for an empty query).
fn better(left: (&Score, &str), right: (&Score, &str), query_empty: bool) -> Ordering {
    let ranked = if query_empty {
        Ordering::Equal
    } else {
        score_order(left.0, right.0).then(left.1.len().cmp(&right.1.len()))
    };
    ranked.then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
}

// fzf's v2 path-scheme score, as fx keeps it (`file_index.zig:1410-1423`).
const MOST_ALIGN_CELLS: usize = 4096;
const RUN_BONUS: [i32; 4] = [4, 7, 8, 9];
const SCORE_MATCH: i32 = 16;
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2;
const BONUS_NON_WORD: i32 = SCORE_MATCH / 2;
const BONUS_CAMEL_123: i32 = BONUS_BOUNDARY + GAP_EXTENSION;
const BONUS_CONSECUTIVE: i32 = -(GAP_START + GAP_EXTENSION);
const FIRST_CHAR_MULTIPLIER: i32 = 2;
const BONUS_BOUNDARY_WHITE: i32 = BONUS_BOUNDARY;
const BONUS_BOUNDARY_DELIMITER: i32 = BONUS_BOUNDARY + 1;
const INVALID: i32 = i32::MIN;
/// What a placement inside the name earns over one that starts in the
/// folder holding it (`name_bonus`, `file_index.zig:1722-1727`).
const NAME_BONUS: i32 = 4;

/// A byte's class (`CharClass`, `charClassOf`, `file_index.zig:1445-1457`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    White,
    NonWord,
    Delimiter,
    Lower,
    Upper,
    Letter,
    Number,
}

fn class(byte: u8) -> Class {
    match byte {
        b'a'..=b'z' => Class::Lower,
        b'A'..=b'Z' => Class::Upper,
        b'0'..=b'9' => Class::Number,
        b' ' | b'\t' | b'\n' | b'\r' => Class::White,
        b'/' => Class::Delimiter,
        0x80..=0xff => Class::Letter,
        _ => Class::NonWord,
    }
}

/// What a character after `prev` earns (`bonusFor`, `file_index.zig:1459-1475`).
fn bonus_for(prev: Class, class: Class) -> i32 {
    use Class::*;
    if matches!(class, Lower | Upper | Letter | Number) {
        match prev {
            White => return BONUS_BOUNDARY_WHITE,
            Delimiter => return BONUS_BOUNDARY_DELIMITER,
            NonWord => return BONUS_BOUNDARY,
            _ => {}
        }
    }
    if (prev == Lower && class == Upper) || (prev != Number && class == Number) {
        return BONUS_CAMEL_123;
    }
    match class {
        NonWord | Delimiter => BONUS_NON_WORD,
        White => BONUS_BOUNDARY_WHITE,
        _ => 0,
    }
}

/// The bonus of scalar `position`, read from the path's own bytes: the case
/// as written, and the start a separator (`bonusAt`, `file_index.zig:1477-1481`).
fn bonus_at(path: &[u8], offsets: &[usize], position: usize) -> i32 {
    let at = offsets[position];
    let prev = if at == 0 {
        Class::Delimiter
    } else {
        class(path[at - 1])
    };
    bonus_for(prev, class(path[at]))
}

fn run_slot(run: i32) -> usize {
    match run.max(BONUS_CONSECUTIVE) {
        4 => 0,
        7 => 1,
        8 => 2,
        9 => 3,
        _ => unreachable!("a bonus is 0, 7, 8 or 9"),
    }
}

/// A character in a run: its bonus and the run's (`consecutiveBonus`,
/// `file_index.zig:1493-1496`).
fn consecutive_bonus(bonus: i32, run: i32) -> (i32, i32) {
    if bonus >= BONUS_BOUNDARY && bonus > run {
        return (bonus, bonus);
    }
    (bonus.max(run.max(BONUS_CONSECUTIVE)), run)
}

/// The score of one placement and its first scalar from `range_start`
/// (`scoreAlignment`, `file_index.zig:1498-1521`).
fn score_alignment(
    path: &[u8],
    offsets: &[usize],
    range_start: usize,
    positions: &[usize],
) -> (i32, usize) {
    let (mut total, mut run) = (0, 0);
    for (index, &position) in positions.iter().enumerate() {
        let bonus = bonus_at(path, offsets, position);
        if index == 0 {
            (total, run) = (SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER, bonus);
        } else if position == positions[index - 1] + 1 {
            let (bonus, next) = consecutive_bonus(bonus, run);
            (total, run) = (total + SCORE_MATCH + bonus, next);
        } else {
            let gap = (position - positions[index - 1] - 1) as i32;
            total += SCORE_MATCH + bonus + GAP_START + (gap - 1) * GAP_EXTENSION;
            run = bonus;
        }
    }
    (total, positions[0] - range_start)
}

fn better_candidate(score: i32, start: usize, best: i32, best_start: usize) -> bool {
    score > best || (score == best && start < best_start)
}

/// One search's text and alignment workspace, reused for every path.
#[derive(Default)]
struct Scratch {
    /// The path's scalars, case-folded.
    text: Vec<char>,
    /// Each scalar's first byte, then the path's length.
    offsets: Vec<usize>,
    align: Align,
}

#[derive(Default)]
struct Align {
    /// Four run slots a cell (`AlignScratch`, `file_index.zig:1427-1438`).
    scores: Vec<i32>,
    starts: Vec<usize>,
    pred_entries: Vec<usize>,
    pred_slots: Vec<usize>,
    columns: Vec<usize>,
    row_offsets: Vec<usize>,
    forward: Vec<usize>,
    backward: Vec<usize>,
    /// The placement found last.
    positions: Vec<usize>,
}

impl Scratch {
    fn load(&mut self, path: &str) {
        self.text.clear();
        self.offsets.clear();
        for (at, scalar) in path.char_indices() {
            self.text.push(fold(scalar));
            self.offsets.push(at);
        }
        self.offsets.push(path.len());
    }
}

impl Align {
    /// Keeps `score` in slot `at` if it beats what is there (`betterCandidate`,
    /// `file_index.zig:1523-1525`): higher, or as high and starting earlier.
    fn offer(&mut self, at: usize, score: i32, start: usize, entry: usize, from: usize) {
        if better_candidate(score, start, self.scores[at], self.starts[at]) {
            self.scores[at] = score;
            self.starts[at] = start;
            self.pred_entries[at] = entry;
            self.pred_slots[at] = from;
        }
    }
}

/// The best placement of `query` in `text[range_start..]` whose last scalar is
/// at or after `end_min`: its score and first scalar, the highest score and
/// then the earliest start, its positions in `align.positions`; past
/// [`MOST_ALIGN_CELLS`], the leftmost one (`alignBest`, `file_index.zig:1527-1720`).
fn align_best(
    text: &[char],
    path: &[u8],
    offsets: &[usize],
    range_start: usize,
    end_min: usize,
    query: &[char],
    s: &mut Align,
) -> Option<(i32, usize)> {
    let m = query.len();
    if m == 0 || range_start > text.len() || m > text.len() - range_start {
        return None;
    }
    s.positions.clear();
    if m == 1 && text.len() - end_min <= MOST_ALIGN_CELLS {
        // The earliest scalar with the highest bonus (`:1546-1564`).
        let mut best: Option<(usize, i32)> = None;
        for (j, _) in text
            .iter()
            .enumerate()
            .skip(end_min)
            .filter(|(_, c)| **c == query[0])
        {
            let bonus = bonus_at(path, offsets, j);
            if best.is_none_or(|(_, top)| bonus > top) {
                best = Some((j, bonus));
                if bonus == BONUS_BOUNDARY_DELIMITER {
                    break;
                }
            }
        }
        let (only, bonus) = best?;
        s.positions.push(only);
        return Some((
            SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER,
            only - range_start,
        ));
    }
    s.forward.clear();
    for (position, scalar) in text.iter().enumerate().skip(range_start) {
        if *scalar == query[s.forward.len()] {
            s.forward.push(position);
            if s.forward.len() == m {
                break;
            }
        }
    }
    if s.forward.len() < m {
        return None;
    }
    s.backward.clear();
    s.backward.resize(m, 0);
    let mut row = m;
    for (position, scalar) in text.iter().enumerate().rev() {
        if *scalar == query[row - 1] {
            row -= 1;
            s.backward[row] = position;
            if row == 0 {
                break;
            }
        }
    }
    if s.backward[m - 1] < end_min {
        return None;
    }
    if s.forward[m - 1] < end_min {
        s.forward[m - 1] = (end_min..text.len()).find(|at| text[*at] == query[m - 1])?;
    }
    let cells: usize = s
        .forward
        .iter()
        .zip(&s.backward)
        .map(|(a, b)| b - a + 1)
        .sum();
    if s.forward == s.backward || cells > MOST_ALIGN_CELLS {
        // One placement, or too many cells: the leftmost (`:1591-1604`).
        s.positions.extend_from_slice(&s.forward);
        return Some(score_alignment(path, offsets, range_start, &s.positions));
    }
    // One cell for each place `query[row]` matches between its two greedy
    // positions, four run slots each.
    s.columns.clear();
    s.row_offsets.clear();
    for (row, wanted) in query.iter().enumerate() {
        s.row_offsets.push(s.columns.len());
        let (first, last) = (s.forward[row], s.backward[row]);
        s.columns
            .extend((first..=last).filter(|at| text[*at] == *wanted));
    }
    s.row_offsets.push(s.columns.len());
    let slots = s.columns.len() * 4;
    for cells in [&mut s.starts, &mut s.pred_entries, &mut s.pred_slots] {
        cells.clear();
        cells.resize(slots, 0);
    }
    s.scores.clear();
    s.scores.resize(slots, INVALID);
    for row in 0..m {
        let prev = if row > 0 {
            s.row_offsets[row - 1]..s.row_offsets[row]
        } else {
            0..0
        };
        let (mut gap, mut gap_start, mut gap_entry, mut gap_slot) = (INVALID, 0, 0, 0);
        let (mut next_gap, mut diag) = (prev.start, prev.start);
        for entry in s.row_offsets[row]..s.row_offsets[row + 1] {
            let (j, cell) = (s.columns[entry], entry * 4);
            let bonus = bonus_at(path, offsets, j);
            if row == 0 {
                let slot = run_slot(bonus);
                s.scores[cell + slot] = SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER;
                s.starts[cell + slot] = j;
                continue;
            }
            // Every predecessor at least two positions back.
            while next_gap < prev.end && s.columns[next_gap] + 2 <= j {
                for slot in 0..4 {
                    let (value, start) =
                        (s.scores[next_gap * 4 + slot], s.starts[next_gap * 4 + slot]);
                    if value == INVALID {
                        continue;
                    }
                    let shifted = value + s.columns[next_gap] as i32;
                    if better_candidate(shifted, start, gap, gap_start) {
                        (gap, gap_start, gap_entry, gap_slot) = (shifted, start, next_gap, slot);
                    }
                }
                next_gap += 1;
            }
            while diag < prev.end && s.columns[diag] + 1 < j {
                diag += 1;
            }
            if diag < prev.end && s.columns[diag] + 1 == j {
                for (from, run) in RUN_BONUS.into_iter().enumerate() {
                    let (value, start) = (s.scores[diag * 4 + from], s.starts[diag * 4 + from]);
                    if value != INVALID {
                        let (step, run) = consecutive_bonus(bonus, run);
                        s.offer(
                            cell + run_slot(run),
                            value + SCORE_MATCH + step,
                            start,
                            diag,
                            from,
                        );
                    }
                }
            }
            if gap != INVALID {
                // A gap of `j - p - 1`: `GAP_START`, then `GAP_EXTENSION`
                // for each position past its first (`:1676-1685`).
                let score = gap - j as i32 - 1 + SCORE_MATCH + bonus;
                s.offer(
                    cell + run_slot(bonus),
                    score,
                    gap_start,
                    gap_entry,
                    gap_slot,
                );
            }
        }
    }
    let (mut best, mut best_start, mut at) = (INVALID, 0, 0);
    for cell in s.row_offsets[m - 1] * 4..s.row_offsets[m] * 4 {
        let (value, start) = (s.scores[cell], s.starts[cell]);
        if value != INVALID && better_candidate(value, start, best, best_start) {
            (best, best_start, at) = (value, start, cell);
        }
    }
    if best == INVALID {
        return None;
    }
    s.positions.resize(m, 0);
    for row in (0..m).rev() {
        s.positions[row] = s.columns[at / 4];
        at = s.pred_entries[at] * 4 + s.pred_slots[at];
    }
    Some((best, best_start - range_start))
}

/// The highest score a placement that starts above the name and ends in it
/// can reach (`maxCrossingQuality`, `file_index.zig:1736-1743`).
fn max_crossing_quality(query: &[char]) -> i32 {
    let m = query.len() as i32;
    let ceiling = SCORE_MATCH * m + BONUS_BOUNDARY_DELIMITER * (m + 1);
    if query.contains(&'/') {
        ceiling
    } else {
        ceiling + GAP_START
    }
}

/// The placement a path is ranked by, and the search that found it
/// (`choosePlacement`, `file_index.zig:1776-1819`): the best inside the name
/// plus [`NAME_BONUS`], or a better one from the folder holding the name into
/// it; failing both, the best anywhere, ranked in the folder tier below every
/// name match whether or not it ends in the name.
fn choose_placement(
    path: &str,
    base: usize,
    query: &[char],
    s: &mut Scratch,
) -> Option<(Score, usize, usize)> {
    let (text, offsets, bytes) = (&s.text[..], &s.offsets[..], path.as_bytes());
    let place = |quality: i32, first: usize| Score {
        quality,
        first,
        name: true,
        ..Score::default()
    };
    let mut best = align_best(text, bytes, offsets, base, base, query, &mut s.align)
        .map(|(quality, first)| (place(quality + NAME_BONUS, first), base, base));
    let can_win = best.is_none_or(|(inside, ..)| inside.quality < max_crossing_quality(query));
    if base > 0 && query.len() > 1 && can_win {
        let folder = text[..base - 1]
            .iter()
            .rposition(|c| *c == '/')
            .map_or(0, |slash| slash + 1);
        if let Some((quality, first)) =
            align_best(text, bytes, offsets, folder, base, query, &mut s.align)
        {
            if best.is_none_or(|(inside, ..)| quality > inside.quality) {
                best = Some((place(quality, first), folder, base));
            }
        }
    }
    best.or_else(|| {
        let (quality, first) = align_best(text, bytes, offsets, 0, 0, query, &mut s.align)?;
        Some((
            Score {
                quality,
                first,
                ..Score::default()
            },
            0,
            0,
        ))
    })
}

/// A query, case-folded once.
struct Folded(Vec<char>);

impl Folded {
    fn new(query: &str) -> Self {
        Self(query.chars().map(fold).collect())
    }
}

/// One scalar's case fold: its lower case when that is one scalar, itself
/// otherwise. Not upstream's `unicode_simple_fold`: that maps `ς` to `σ`,
/// which lower-casing leaves apart, so `σigma` finds `ςigma` in fx and not
/// here (`file_index.zig:2909-2944@481104fd`). Unit CASE-FOLD-SIMPLE, queued.
fn fold(scalar: char) -> char {
    if scalar.is_ascii() {
        return scalar.to_ascii_lowercase();
    }
    let mut lower = scalar.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => scalar,
    }
}

/// Whether `path` holds `query`'s scalars in order at all: one pass, which
/// most paths fail, before any is scored. fx checks the same in
/// `choosePlacement` once the name has no placement (`isSubsequence`,
/// `file_index.zig:1804@481104fd`); here it comes first, so
/// [`choose_placement`] needs no second check.
fn holds(path: &str, query: &[char]) -> bool {
    let mut wanted = query.iter().peekable();
    path.chars()
        .any(|scalar| wanted.next_if_eq(&&fold(scalar)).is_some() && wanted.peek().is_none())
}

/// The basename's first byte: after the last `/`.
fn basename_start(path: &str) -> usize {
    path.rfind('/').map_or(0, |slash| slash + 1)
}

/// `path` loaded into `s`, and its name's first scalar.
fn loaded(path: &str, s: &mut Scratch) -> usize {
    s.load(path);
    let base = basename_start(path);
    s.offsets
        .iter()
        .position(|at| *at >= base)
        .unwrap_or(s.text.len())
}

/// How well `query` matches `path` (`scoreAsciiMatchWith`,
/// `scoreFoldedMatchWith`, `file_index.zig:1850-1882`).
fn score(path: &str, query: &Folded, s: &mut Scratch) -> Option<Score> {
    let query = &query.0[..];
    if query.is_empty() || path.is_empty() || !holds(path, query) {
        return None;
    }
    let base = loaded(path, s);
    let (whole, name) = (&s.text[..], &s.text[base..]);
    // `exactFit` (`file_index.zig:1753-1758`): the path, the name, or the
    // name before a dot.
    let exact = whole == query
        || name == query
        || (name.len() > query.len() && name.starts_with(query) && name[query.len()] == '.');
    let (score, ..) = choose_placement(path, base, query, s)?;
    Some(Score { exact, ..score })
}

/// The byte ranges of `path` the query matched -- the placement that set its
/// rank -- whole grapheme clusters, adjacent ones merged
/// (`reconstructMatchSpans` and `spansFromMatchedOffsets`, `file_index.zig:1921-1983`).
fn spans(path: &str, query: &Folded, s: &mut Scratch) -> Vec<Range<usize>> {
    let query = &query.0[..];
    if query.is_empty() || path.is_empty() {
        return Vec::new();
    }
    let base = loaded(path, s);
    let Some((_, range_start, end_min)) = choose_placement(path, base, query, s) else {
        return Vec::new();
    };
    // The chosen search again: a later one in the choice reused the positions.
    if align_best(
        &s.text,
        path.as_bytes(),
        &s.offsets,
        range_start,
        end_min,
        query,
        &mut s.align,
    )
    .is_none()
    {
        return Vec::new();
    }
    let offsets: Vec<usize> = s
        .align
        .positions
        .iter()
        .map(|position| s.offsets[*position])
        .collect();
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut next = offsets.iter().peekable();
    for (start, cluster) in path.grapheme_indices(true) {
        let end = start + cluster.len();
        let mut hit = false;
        while next.peek().is_some_and(|offset| **offset < end) {
            next.next();
            hit = true;
        }
        if !hit {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => out.push(start..end),
        }
    }
    out
}

/// The [`MOST_RESULTS`] best of `entries` for `query`, best first
/// (`searchAtRevision`, `file_index.zig:585-645@481104fd`).
///
/// An empty query is the first entries in the index's own order and matches
/// nothing in particular, so no row has a bold run.
pub(crate) fn search(entries: &[Candidate], query: &str) -> Vec<Found> {
    if query.is_empty() {
        return entries
            .iter()
            .take(MOST_RESULTS)
            .map(|candidate| Found {
                path: candidate.path.clone(),
                kind: candidate.kind,
                spans: Vec::new(),
            })
            .collect();
    }
    let folded = Folded::new(query);
    let mut scratch = Scratch::default();
    // Room for every entry, so a query that matches most of a large index
    // never grows it; fx keeps a bounded top set instead (AT-RANK-PERF).
    let mut ranked: Vec<(Score, usize)> = Vec::with_capacity(entries.len());
    for (index, candidate) in entries.iter().enumerate() {
        if let Some(score) = score(&candidate.path, &folded, &mut scratch) {
            ranked.push((score, index));
        }
    }
    let order = |left: &(Score, usize), right: &(Score, usize)| {
        better(
            (&left.0, &entries[left.1].path),
            (&right.0, &entries[right.1].path),
            false,
        )
    };
    if ranked.len() > MOST_RESULTS {
        ranked.select_nth_unstable_by(MOST_RESULTS - 1, order);
        ranked.truncate(MOST_RESULTS);
    }
    ranked.sort_by(order);
    ranked
        .into_iter()
        .map(|(_, index)| {
            let candidate = &entries[index];
            Found {
                path: candidate.path.clone(),
                kind: candidate.kind,
                spans: spans(&candidate.path, &folded, &mut scratch),
            }
        })
        .collect()
}
