//! A skill named for one query: which ones the prompt names, the system block
//! that carries their instructions whole, and the `skill` tool's load.
//!
//! Upstream's (`vercel-labs/fx@c1db9191 src/core/skills/skill_invocation.zig`,
//! selection in `skill_runtime.zig:2683-2962`). A prompt names a skill three
//! ways -- `$name` anywhere outside quotes, code and `@` paths; `/name` at its
//! very start; or "use the name skill" as its opening words -- and a skill the
//! composer bound by its `$` token is named by that binding, exact path and
//! all. Every skill named is loaded **whole**, for this query only, into one
//! system block: `Explicitly invoked skill content for this query:`, four
//! sentences, and one `<skill_content …>` element per skill.
//!
//! The block is ephemeral, as upstream's is (`processQueuedPromptLoop`'s
//! `ephemeral_overlay`, `orchestrator.zig:6929-6932`): every request of the
//! turn carries it and none of the turns after does, unless their own prompt
//! names the skill again.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::catalog::{write_scalar, Locations};
use super::contract;
use super::discovery::{self, Diagnostic, Reopened, Skill};

/// The most of one resource a load reads (`skill_file_bytes`' compiled
/// default, `context_limits.zig:23`).
pub const FILE_LIMIT: usize = 1024 * 1024;

/// The block's opening lines (`buildExplicitPromptSection`, `:231-236`).
const EXPLICIT_HEADER: &str = "Explicitly invoked skill content for this query:\n\
Use every successfully loaded skill for this query. Report blocked or ambiguous requests.\n\
Follow each skill's complete instructions and required resources before substantive work.\n\
If a skill cannot be followed, state the blocker instead of silently substituting another workflow.\n";

/// What leads a tool result when discovery skipped something
/// (`discovery_model_notice`, `:17`).
const DISCOVERY_MODEL_NOTICE: &str = "<skill_discovery_warning details=\"context_notice\" />\n";

/// The budget an ambiguous name's refusal is written to inside the block
/// (`formatAmbiguousSkill(..., 4096)`, `:257`).
const AMBIGUOUS_BUDGET: usize = 4096;

/// A skill the composer bound by its token: the name it showed, and the exact
/// directory it stood for (`ExplicitBinding`, `:186-190`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub path: PathBuf,
}

/// One skill a prompt names: an index into the catalog, or a name that more
/// than one skill answers to (`ExplicitSelection`, `skill_runtime.zig:2679-2681`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Skill(usize),
    Ambiguous(String),
}

/// Every skill `prompt` names, in the order it names them, each once
/// (`collectExplicitSkillSelections`, `skill_runtime.zig:2696-2797`).
pub fn selections(prompt: &str, skills: &[Skill]) -> Vec<Selection> {
    let mut out: Vec<Selection> = Vec::new();
    let text = prompt.as_bytes();
    let trimmed = prompt.trim_start_matches([' ', '\t', '\r', '\n']);
    // The words before the first `@` path or `$` are the only ones read as
    // "use the X skill".
    let natural_end = (0..text.len())
        .find(|index| at_word_end(text, *index).is_some() || text[*index] == b'$')
        .unwrap_or(text.len());
    let natural = NaturalReference::parse(&prompt[..natural_end]);
    for (index, skill) in skills.iter().enumerate() {
        let named = sigil_at(trimmed.as_bytes(), 0, &skill.name, b'/')
            || natural
                .as_ref()
                .is_some_and(|reference| reference.matches(&skill.name));
        if !named {
            continue;
        }
        let duplicates = skills
            .iter()
            .filter(|other| other.name.eq_ignore_ascii_case(&skill.name))
            .count();
        push(
            &mut out,
            if duplicates == 1 {
                Selection::Skill(index)
            } else {
                Selection::Ambiguous(skill.name.clone())
            },
        );
    }

    let mut quote: Option<u8> = None;
    let mut code: Option<(u8, usize, bool)> = None;
    let mut index = 0usize;
    while index < text.len() {
        let byte = text[index];
        if let Some((open, len, fenced)) = code {
            if byte == open {
                let end = run_end(text, index);
                let closes = if fenced {
                    end - index >= len && fence_line_start(text, index) && {
                        let rest = &text[end..];
                        let line_end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
                        trim_bytes(&rest[..line_end]).is_empty()
                    }
                } else {
                    end - index == len
                };
                if closes {
                    code = None;
                }
                index = end;
                continue;
            }
            index += 1;
            continue;
        }
        if byte == b'\\' {
            index += if index + 1 < text.len() { 2 } else { 1 };
            continue;
        }
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        if let Some(end) = at_word_end(text, index) {
            index = end.max(index + 1);
            continue;
        }
        if byte == b'`' || byte == b'~' {
            let end = run_end(text, index);
            let fenced = end - index >= 3 && fence_line_start(text, index) && {
                let rest = &text[end..];
                let line_end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
                byte == b'~' || !rest[..line_end].contains(&b'`')
            };
            if fenced || byte == b'`' {
                code = Some((byte, end - index, fenced));
            }
            index = end;
            continue;
        }
        if byte == b'"'
            || (byte == b'\'' && (index == 0 || !text[index - 1].is_ascii_alphanumeric()))
        {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if text[index..].starts_with("\u{201c}".as_bytes())
            || text[index..].starts_with("\u{2018}".as_bytes())
        {
            let close = if text[index..].starts_with("\u{201c}".as_bytes()) {
                "\u{201d}"
            } else {
                "\u{2019}"
            };
            let Some(next) = find(&text[index + 3..], close.as_bytes()) else {
                break;
            };
            index += 3 + next + close.len();
            continue;
        }
        if byte != b'$' || negated(&prompt[..index]) {
            index += 1;
            continue;
        }
        let mut found: Option<usize> = None;
        let mut longest = 0usize;
        let mut ambiguous = false;
        for (skill_index, skill) in skills.iter().enumerate() {
            if !sigil_at(text, index, &skill.name, b'$') || skill.name.len() < longest {
                continue;
            }
            if skill.name.len() > longest {
                found = Some(skill_index);
                longest = skill.name.len();
                ambiguous = false;
            } else {
                ambiguous = true;
            }
        }
        match found {
            Some(skill_index) => {
                push(
                    &mut out,
                    if ambiguous {
                        Selection::Ambiguous(skills[skill_index].name.clone())
                    } else {
                        Selection::Skill(skill_index)
                    },
                );
                index += longest + 1;
            }
            None => index += 1,
        }
    }
    out
}

fn push(out: &mut Vec<Selection>, selection: Selection) {
    let already = out.iter().any(|existing| match (existing, &selection) {
        (Selection::Skill(left), Selection::Skill(right)) => left == right,
        (Selection::Ambiguous(left), Selection::Ambiguous(right)) => {
            left.eq_ignore_ascii_case(right)
        }
        _ => false,
    });
    if !already {
        out.push(selection);
    }
}

/// Whether `text` at `index` is `sigil` and then `name`, whole: not followed by
/// a letter, a digit, `_` or `-` (`matchesSigilSkillAt`, `:2907-2914`).
fn sigil_at(text: &[u8], index: usize, name: &str, sigil: u8) -> bool {
    if text.get(index) != Some(&sigil) {
        return false;
    }
    let start = index + 1;
    let end = start + name.len();
    if end > text.len() || !text[start..end].eq_ignore_ascii_case(name.as_bytes()) {
        return false;
    }
    text.get(end)
        .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-'))
}

/// Whether the words before a `$` say not to use what follows
/// (`explicitReferenceNegated`, `:2819-2833`).
fn negated(before: &str) -> bool {
    const ENDINGS: [&str; 22] = [
        "not",
        "don't",
        "without",
        "not use",
        "don't use",
        "not apply",
        "don't apply",
        "not invoke",
        "don't invoke",
        "not run",
        "don't run",
        "not activate",
        "don't activate",
        "not use the",
        "don't use the",
        "never",
        "never use",
        "never apply",
        "never invoke",
        "never run",
        "never activate",
        "never use the",
    ];
    let trimmed = before
        .trim_end_matches([' ', '\t', '\r', '\n', ',', ':'])
        .as_bytes();
    ENDINGS.iter().any(|ending| {
        let ending = ending.as_bytes();
        if trimmed.len() < ending.len() {
            return false;
        }
        let start = trimmed.len() - ending.len();
        if start > 0 && trimmed[start - 1].is_ascii_alphanumeric() {
            return false;
        }
        trimmed[start..].eq_ignore_ascii_case(ending)
    })
}

/// The run of the same byte starting at `start`, as an end.
fn run_end(text: &[u8], start: usize) -> usize {
    let mut end = start + 1;
    while end < text.len() && text[end] == text[start] {
        end += 1;
    }
    end
}

/// Whether `index` begins a line, up to three spaces in.
fn fence_line_start(text: &[u8], index: usize) -> bool {
    let mut start = index;
    while start > 0 && index - start < 3 && text[start - 1] == b' ' {
        start -= 1;
    }
    start == 0 || text[start - 1] == b'\n'
}

fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\r'))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Where an `@` path word that starts at `start` ends, when one does
/// (`file_picker_path.parse_at`, `vercel-labs/fx@c1db9191
/// src/core/input/file_picker_path.zig:69-115`): the same word the `@` file
/// picker reads, so a `$` inside a path names nothing.
fn at_word_end(text: &[u8], start: usize) -> Option<usize> {
    let separator = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
    if text.get(start) != Some(&b'@') {
        return None;
    }
    if start > 0
        && !(separator(text[start - 1])
            || matches!(
                text[start - 1],
                b'(' | b'[' | b'{' | b'<' | b'\'' | b'"' | b'`'
            ))
    {
        return None;
    }
    let token_end = |from: usize| {
        let mut index = from;
        let mut quote: Option<u8> = None;
        while index < text.len() {
            let byte = text[index];
            if byte == b'\\' && index + 1 < text.len() {
                index += 2;
                continue;
            }
            match quote {
                Some(open) if byte == open => quote = None,
                Some(_) => {}
                None if byte == b'"' || byte == b'\'' => quote = Some(byte),
                None if separator(byte) => break,
                None => {}
            }
            index += 1;
        }
        index
    };
    let quoted = text.get(start + 1) == Some(&b'"');
    let path_start = start + 1 + usize::from(quoted);
    if !quoted {
        if text.get(path_start) == Some(&b'\'') {
            return Some(token_end(path_start));
        }
        let mut end = path_start;
        while end < text.len() && !separator(text[end]) {
            if text[end] == b'\\' && end + 1 < text.len() {
                end += 1;
            }
            end += 1;
        }
        return Some(end.min(text.len()));
    }
    let mut index = path_start;
    while index < text.len() {
        let byte = text[index];
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            break;
        }
        if byte == b'\\' {
            if index + 1 == text.len() {
                return Some(text.len());
            }
            if matches!(text[index + 1], b'\n' | b'\r' | b'\t') {
                break;
            }
            index += 2;
            continue;
        }
        if byte == b'"' {
            return Some(token_end(index + 1));
        }
        index += 1;
    }
    Some(index)
}

/// "use the X skill" at the start of a prompt (`NaturalLanguageSkillReference`,
/// `:2835-2905`).
struct NaturalReference {
    normalized: String,
    starts: Vec<usize>,
}

impl NaturalReference {
    fn parse(prompt: &str) -> Option<Self> {
        let mut text = prompt.trim_start_matches([' ', '\t', '\r', '\n']);
        let bytes = text.as_bytes();
        if bytes.len() >= 6
            && bytes[..6].eq_ignore_ascii_case(b"please")
            && bytes
                .get(6)
                .is_none_or(|byte| !byte.is_ascii_alphanumeric())
        {
            text = text[6..].trim_start_matches([' ', '\t', '\r', '\n', ',', ':']);
        }
        if text.is_empty()
            || text.starts_with(['"', '\'', '`'])
            || text.starts_with('\u{201c}')
            || text.starts_with('\u{2018}')
        {
            return None;
        }
        let normalized = normalize(text);
        let start = ["use", "apply", "activate", "invoke", "run"]
            .iter()
            .find_map(|verb| {
                (normalized.starts_with(verb)
                    && normalized.as_bytes().get(verb.len()) == Some(&b' '))
                .then_some(verb.len() + 1)
            })?;
        let mut starts = vec![start];
        if normalized[start..].starts_with("the ") {
            starts.push(start + 4);
        }
        Some(Self { normalized, starts })
    }

    fn matches(&self, name: &str) -> bool {
        for &start in &self.starts {
            let mut search = start;
            while let Some(relative) = self.normalized[search..].find(" skill") {
                let marker = search + relative;
                let end = marker + " skill".len();
                let boundary =
                    end == self.normalized.len() || self.normalized.as_bytes()[end] == b' ';
                if boundary
                    && marker > start
                    && normalized_equal(name, &self.normalized[start..marker])
                {
                    return true;
                }
                search = marker + 1;
            }
        }
        false
    }
}

/// Letters and digits lowercased, every other run one space, no space at
/// either end (`normalizeReferenceText`, `:2947-2962`).
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_space = true;
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(char::from(byte.to_ascii_lowercase()));
            previous_space = false;
        } else if !previous_space {
            out.push(' ');
            previous_space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Whether `name`, normalized, is `normalized` (`normalizedReferenceTextEql`).
fn normalized_equal(name: &str, normalized: &str) -> bool {
    let wanted = normalized.as_bytes();
    let mut at = 0usize;
    let mut pending = false;
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() {
            if pending && at > 0 {
                if wanted.get(at) != Some(&b' ') {
                    return false;
                }
                at += 1;
            }
            if wanted.get(at) != Some(&byte.to_ascii_lowercase()) {
                return false;
            }
            at += 1;
            pending = false;
        } else if at > 0 {
            pending = true;
        }
    }
    at == wanted.len()
}

/// What one query's explicit skills came to: the block, the row the
/// transcript says, and the details a failure leaves for the full transcript.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Explicit {
    /// The system block, or empty when the query names no skill.
    pub text: String,
    /// `N requested skill(s) loaded` and its `├`/`└` rows, or the failure
    /// count, and whether it is a warning (a skill failed to load).
    pub load_notice: Option<(bool, String)>,
    /// What each failure was, for the full transcript.
    pub details: Option<String>,
    /// Context notices (a sanitized load) and the discovery warning.
    pub notice: Option<String>,
    pub diagnostic_notice: Option<String>,
}

/// The ordered plan: the composer's bindings first, then what the prompt
/// names, each directory once (`buildExplicitBindingPlan`, `:535-572`).
enum Planned {
    Bound { name: String, path: PathBuf },
    Ambiguous(String),
}

/// The block for one query (`buildExplicitPromptSection`, `:205-289`).
pub fn explicit(
    skills: &[Skill],
    diagnostics: &[Diagnostic],
    prompt: &str,
    bindings: &[Binding],
) -> Explicit {
    let mut plan: Vec<Planned> = Vec::new();
    let mut loaded_paths: Vec<PathBuf> = Vec::new();
    for binding in bindings {
        if loaded_paths.contains(&binding.path) {
            continue;
        }
        loaded_paths.push(binding.path.clone());
        plan.push(Planned::Bound {
            name: binding.name.clone(),
            path: binding.path.clone(),
        });
    }
    for selection in selections(prompt, skills) {
        match selection {
            Selection::Skill(index) => {
                let skill = &skills[index];
                if loaded_paths.contains(&skill.path) {
                    continue;
                }
                loaded_paths.push(skill.path.clone());
                plan.push(Planned::Bound {
                    name: skill.name.clone(),
                    path: skill.path.clone(),
                });
            }
            Selection::Ambiguous(name) => {
                if !bindings
                    .iter()
                    .any(|binding| binding.name.eq_ignore_ascii_case(&name))
                {
                    plan.push(Planned::Ambiguous(name));
                }
            }
        }
    }
    if plan.is_empty() {
        return Explicit::default();
    }

    let mut text = String::from(EXPLICIT_HEADER);
    let mut rows = String::new();
    let mut details = String::new();
    let mut notices = String::new();
    let mut diagnostic_notice: Option<String> = None;
    let mut loaded = 0usize;
    let count = plan.len();
    for (index, entry) in plan.into_iter().enumerate() {
        rows.push_str(if index + 1 == count {
            "\n\u{2514} "
        } else {
            "\n\u{251c} "
        });
        match entry {
            Planned::Bound { name, path } => {
                let result = load_by_identity(skills, diagnostics, &name, Some(&path), None, None);
                text.push_str(&result.output);
                text.push('\n');
                if let Some(notice) = &result.notice {
                    notices.push_str(notice);
                    if !notice.ends_with('\n') {
                        notices.push('\n');
                    }
                }
                if diagnostic_notice.is_none() {
                    diagnostic_notice = result.diagnostic_notice.clone();
                }
                let failure = if result.complete {
                    None
                } else if result.ok {
                    Some("Complete instructions were not loaded.".to_string())
                } else {
                    Some(result.output.clone())
                };
                if let Some(detail) = &failure {
                    details.push_str(&load_row(&name, Some(detail)));
                    details.push('\n');
                }
                rows.push_str(&load_row(&name, failure.as_ref().map(|_| "")));
                if failure.is_none() {
                    loaded += 1;
                }
            }
            Planned::Ambiguous(name) => {
                let failure = ambiguous(skills, &name, AMBIGUOUS_BUDGET);
                text.push_str(&failure);
                text.push('\n');
                rows.push_str(&load_row(&name, Some("ambiguous name")));
                details.push_str(&load_row(&name, Some(&failure)));
                details.push('\n');
            }
        }
    }
    let failed = count - loaded;
    let summary = if failed == 0 {
        format!(
            "{loaded} requested skill{} loaded{rows}",
            if loaded == 1 { "" } else { "s" }
        )
    } else {
        format!("Requested skills \u{b7} {loaded} loaded \u{b7} {failed} failed (ctrl+o for details){rows}")
    };
    Explicit {
        text,
        load_notice: Some((failed > 0, summary)),
        details: (!details.is_empty()).then_some(details),
        notice: (!notices.is_empty()).then_some(notices),
        diagnostic_notice,
    }
}

/// One row of the load notice (`appendExplicitLoadRow`, `:291-305`).
fn load_row(name: &str, failure: Option<&str>) -> String {
    match failure {
        Some(detail) if !detail.is_empty() => format!("Could not load {name}: {detail}"),
        Some(_) => format!("Could not load {name}"),
        None => format!("Loaded skill {name}"),
    }
}

/// What one load came to (`ExecuteResult`, `:154-178`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// Whether it loaded: a failure is a sentence for the model.
    pub ok: bool,
    /// What the model is given.
    pub output: String,
    /// Whether the complete instructions were loaded.
    pub complete: bool,
    pub notice: Option<String>,
    pub diagnostic_notice: Option<String>,
}

impl Loaded {
    fn failure(output: String) -> Self {
        Self {
            ok: false,
            output,
            complete: false,
            notice: None,
            diagnostic_notice: None,
        }
    }
}

/// Loads the skill named `name` (at `location`, when given) from `skills`,
/// whole, as the block and the tool's location form do
/// (`loadByIdentityWithOptions` in `.whole` mode, `:770-920`).
///
/// `budget` is the tool result's byte limit; the block has none.
pub fn load_by_identity(
    skills: &[Skill],
    diagnostics: &[Diagnostic],
    name: &str,
    location: Option<&Path>,
    resource: Option<&str>,
    budget: Option<usize>,
) -> Loaded {
    load(
        skills,
        diagnostics,
        name,
        location,
        resource,
        budget,
        Mode::Whole,
    )
}

/// The most of a resource one call of the tool's legacy named form returns
/// (`skill_chunk_bytes`' compiled default, `context_limits.zig:22`).
pub const CHUNK_LIMIT: usize = 20 * 1024;

/// How much of a resource a load returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// All of it in one `complete="true"` element, or nothing.
    Whole,
    /// The tool's legacy named form: [`CHUNK_LIMIT`] of it from this byte on,
    /// cut at a line break, and where the next call takes up.
    Chunk { offset: usize },
}

/// The tool's legacy named form, `name` with an optional `location`,
/// `resource` and `offset`: the resource a chunk at a time
/// (`loadByIdentityWithOptions` in `.chunk` mode, `:920-998`).
pub fn load_chunk(
    skills: &[Skill],
    diagnostics: &[Diagnostic],
    name: &str,
    location: Option<&Path>,
    resource: Option<&str>,
    offset: usize,
    budget: Option<usize>,
) -> Loaded {
    load(
        skills,
        diagnostics,
        name,
        location,
        resource,
        budget,
        Mode::Chunk { offset },
    )
}

fn load(
    skills: &[Skill],
    diagnostics: &[Diagnostic],
    name: &str,
    location: Option<&Path>,
    resource: Option<&str>,
    budget: Option<usize>,
    mode: Mode,
) -> Loaded {
    let resource = match resource {
        Some(resource) if !resource.is_empty() => resource,
        _ => "SKILL.md",
    };
    let mut discovery_notice =
        (!diagnostics.is_empty()).then(|| super::diagnostic_summary(diagnostics));
    let primary = budget.map(|limit| {
        if discovery_notice.is_some() {
            limit.saturating_sub(DISCOVERY_MODEL_NOTICE.len())
        } else {
            limit
        }
    });
    let bound = primary.unwrap_or(usize::MAX);
    let skill = match resolve(skills, name, location) {
        Resolution::Found(skill) => skill,
        Resolution::NotFound => {
            let output = match location {
                Some(path) => exact_not_found(name, &path.to_string_lossy(), bound),
                None => missing(name, bound),
            };
            return attach(Loaded::failure(output), &mut discovery_notice, budget);
        }
        Resolution::Ambiguous => {
            return attach(
                Loaded::failure(ambiguous(skills, name, bound)),
                &mut discovery_notice,
                budget,
            )
        }
        Resolution::Mismatch => {
            let path = location
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            return attach(
                Loaded::failure(location_mismatch(name, &path, bound)),
                &mut discovery_notice,
                budget,
            );
        }
    };
    let authority = discovery::authority_of(skill);
    let mut candidate = match discovery::reopen(skill, authority.as_deref()) {
        Ok(candidate) => candidate,
        Err(Reopened::NameMismatch) => {
            let output = match location {
                Some(path) => location_mismatch(name, &path.to_string_lossy(), bound),
                None => missing(name, bound),
            };
            return attach(Loaded::failure(output), &mut discovery_notice, budget);
        }
        Err(Reopened::Missing | Reopened::Skipped) => {
            let output = match location {
                Some(path) => exact_not_found(name, &path.to_string_lossy(), bound),
                None => missing(name, bound),
            };
            return attach(Loaded::failure(output), &mut discovery_notice, budget);
        }
    };
    #[cfg(test)]
    tests::after_validation();
    // A resource other than the skill's own file is read only while the
    // skill is still the one that was checked: its `SKILL.md`, held open, is
    // read again and must not have changed under the read
    // (`revalidatePrimaryIdentity`, `skill_invocation.zig:1316-1329`, from
    // `:857-859`), in either mode.
    let read = match read_resource(&mut candidate, resource).and_then(|read| {
        if resource_is_skill_file(resource) {
            Ok(read)
        } else {
            revalidate_primary(&candidate.file, skill).map(|()| read)
        }
    }) {
        Ok(read) => read,
        Err(error) => {
            return attach(
                Loaded::failure(format!("skill failed: {error}")),
                &mut discovery_notice,
                budget,
            )
        }
    };
    if let Mode::Chunk { offset } = mode {
        return attach(
            chunk(skill, resource, &read, offset),
            &mut discovery_notice,
            budget,
        );
    }
    if read.observed > read.bytes.len() {
        let blocked = file_blocked(resource, read.observed);
        let mut failure = Loaded::failure(blocked.clone());
        failure.notice = Some(blocked);
        return attach(failure, &mut discovery_notice, budget);
    }
    if resource_is_skill_file(resource) {
        let fallback = skill
            .path
            .file_name()
            .map(|leaf| leaf.to_string_lossy().into_owned())
            .unwrap_or_default();
        match contract::resolve(&contract::parse(&read.bytes), &fallback) {
            Ok(metadata) if metadata.name == skill.name => {}
            _ => {
                return attach(
                    Loaded::failure("skill failed: SkillResourceChanged".to_string()),
                    &mut discovery_notice,
                    budget,
                )
            }
        }
    }
    // Checked to be UTF-8 on the way in ([`read_resource`]).
    let body = String::from_utf8_lossy(&read.bytes);
    let mut output = String::from("<skill_content name=\"");
    write_scalar(&mut output, &skill.name);
    output.push_str("\" location=\"");
    write_scalar(&mut output, &skill.path.to_string_lossy());
    output.push_str("\" resource=\"");
    write_scalar(&mut output, resource);
    output.push_str("\" complete=\"true\">\n");
    output.push_str(&body);
    output.push_str("\n</skill_content>");
    // Text with a NUL in it is not shown to a model as it is
    // (`prepareSanitizedOutput`): the whole load is replaced by its size.
    let sanitized = if output.contains('\0') {
        format!(
            "binary or non-utf8 tool output omitted ({} bytes)",
            output.len()
        )
    } else {
        output.clone()
    };
    if sanitized.len() > bound {
        let message = format!(
            "Complete skill content exceeds max_tool_result_bytes ({bound} bytes). No complete instructions were loaded."
        );
        return attach(
            Loaded::failure(clip(&message, bound)),
            &mut discovery_notice,
            budget,
        );
    }
    let notice = (sanitized != output)
        .then(|| "[context] Skill content was sanitized before delivery.\n".to_string());
    attach(
        Loaded {
            ok: true,
            output: sanitized,
            complete: true,
            notice,
            diagnostic_notice: None,
        },
        &mut discovery_notice,
        budget,
    )
}

/// One chunk of `read` from `offset` (`loadByIdentityWithOptions`' chunk arm,
/// `:920-998`): up to [`CHUNK_LIMIT`] bytes cut back to a line break, in a
/// `<skill_content … offset="…" next_offset="…">` element, with a marker and
/// a notice saying where the next call takes up -- or that the rest of the
/// file is past the file limit.
fn chunk(skill: &Skill, resource: &str, read: &Read_, offset: usize) -> Loaded {
    let bytes = &read.bytes;
    if offset > bytes.len() || std::str::from_utf8(&bytes[..offset]).is_err() {
        return Loaded::failure(
            "skill offset must be at a valid UTF-8 boundary within the selected resource"
                .to_string(),
        );
    }
    let blocked = read.observed > bytes.len();
    if offset == bytes.len() && blocked {
        let marker = file_blocked(resource, read.observed);
        let mut failure = Loaded::failure(marker.clone());
        failure.notice = Some(marker);
        return failure;
    }
    let remaining = &bytes[offset..];
    let length = line_safe_prefix(remaining, CHUNK_LIMIT);
    if !remaining.is_empty() && length == 0 {
        let mut marker =
            String::from("<context_limit name=\"skill_chunk_bytes\" action=\"blocked\" skill=\"");
        write_scalar(&mut marker, &skill.name);
        marker.push_str("\" resource=\"");
        write_scalar(&mut marker, resource);
        marker.push_str(&format!(
            "\" observed_bytes=\"{}\" effective_bytes=\"{CHUNK_LIMIT}\" source=\"compiled default\" offset=\"{offset}\" override=\"--context-limit skill_chunk_bytes=BYTES|off\" />",
            remaining.len()
        ));
        let mut failure = Loaded::failure(marker.clone());
        failure.notice = Some(marker);
        return failure;
    }
    let next = offset + length;
    let truncated = next < bytes.len();
    let file_ended = next == bytes.len() && blocked;
    let mut output = String::from("<skill_content name=\"");
    write_scalar(&mut output, &skill.name);
    output.push_str("\" resource=\"");
    write_scalar(&mut output, resource);
    output.push_str(&format!("\" offset=\"{offset}\" next_offset=\"{next}\">\n"));
    // Checked to be UTF-8 on the way in ([`read_resource`]), and cut at a
    // character ([`line_safe_prefix`]).
    output.push_str(&String::from_utf8_lossy(&remaining[..length]));
    if truncated {
        output.push_str(&format!(
            "\n<context_limit name=\"skill_chunk_bytes\" action=\"truncated\" observed_bytes=\"{}\" effective_bytes=\"{CHUNK_LIMIT}\" source=\"compiled default\" next_offset=\"{next}\" override=\"--context-limit skill_chunk_bytes=BYTES|off\" />",
            remaining.len()
        ));
    } else if file_ended {
        output.push('\n');
        output.push_str(&file_blocked(resource, read.observed));
    }
    output.push_str("\n</skill_content>");
    let mut named = String::new();
    write_scalar(&mut named, &skill.name);
    named.push('/');
    write_scalar(&mut named, resource);
    let notice = if truncated {
        Some(format!(
            "[context] skill resource \"{named}\" truncated: observed={} bytes effective={CHUNK_LIMIT} bytes source=compiled default; continue with offset={next} or override with --context-limit skill_chunk_bytes=BYTES|off",
            remaining.len()
        ))
    } else if file_ended {
        Some(format!(
            "[context] skill resource \"{named}\" remainder blocked: observed={} bytes effective={FILE_LIMIT} bytes source=compiled default; override with --context-limit skill_file_bytes=BYTES|off",
            read.observed
        ))
    } else {
        None
    };
    Loaded {
        ok: true,
        output,
        complete: false,
        notice,
        diagnostic_notice: None,
    }
}

/// A result with the discovery warning put in front of it, for the model, and
/// beside it, for the person (`attachExecuteNotice`, `:1022-1058`).
fn attach(mut loaded: Loaded, notice: &mut Option<String>, budget: Option<usize>) -> Loaded {
    let Some(summary) = notice.take() else {
        return loaded;
    };
    let output = match budget {
        Some(limit) => {
            let lead = &DISCOVERY_MODEL_NOTICE[..DISCOVERY_MODEL_NOTICE.len().min(limit)];
            format!(
                "{lead}{}",
                clip(&loaded.output, limit.saturating_sub(lead.len()))
            )
        }
        None => format!("{DISCOVERY_MODEL_NOTICE}{}", loaded.output),
    };
    loaded.output = output;
    loaded.diagnostic_notice = Some(summary);
    loaded
}

/// `text` cut to `limit` bytes at a character boundary.
fn clip(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_string()
}

/// Which advertised skill a name and a location mean (`resolveSkill`,
/// `skill_runtime.zig:1075-1092`).
enum Resolution<'a> {
    Found(&'a Skill),
    NotFound,
    Ambiguous,
    Mismatch,
}

fn resolve<'a>(skills: &'a [Skill], name: &str, location: Option<&Path>) -> Resolution<'a> {
    if let Some(location) = location {
        return match skills.iter().find(|skill| skill.path == location) {
            Some(skill) if skill.name == name => Resolution::Found(skill),
            Some(_) => Resolution::Mismatch,
            None => Resolution::NotFound,
        };
    }
    let mut named = skills.iter().filter(|skill| skill.name == name);
    match (named.next(), named.next()) {
        (Some(skill), None) => Resolution::Found(skill),
        (Some(_), Some(_)) => Resolution::Ambiguous,
        (None, _) => Resolution::NotFound,
    }
}

/// One resource of a skill, read up to [`FILE_LIMIT`] and how big it really is.
struct Read_ {
    bytes: Vec<u8>,
    observed: usize,
}

/// Whether `resource` is the skill's own `SKILL.md` (`resourceIsSkillFile`,
/// `skill_runtime.zig:100-106`).
fn resource_is_skill_file(resource: &str) -> bool {
    let trimmed = resource.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return false;
    }
    let mut segments = trimmed
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty());
    segments.next() == Some("SKILL.md") && segments.next().is_none()
}

/// Reads `resource` of the checked candidate: its `SKILL.md` from the file
/// already opened, or another file under the directory held open, every
/// component opened without following a link, `.` and `..` refused
/// (`openResource`, `skill_runtime.zig:56-90`; `readSkillResourceFile`,
/// `skill_invocation.zig:1371-1409`). A file whose size or modification time
/// moved while it was read is refused as `SkillResourceChanged`.
fn read_resource(
    candidate: &mut discovery::Candidate,
    resource: &str,
) -> Result<Read_, &'static str> {
    let mut opened;
    let file: &mut File = if resource_is_skill_file(resource) {
        &mut candidate.file
    } else {
        let trimmed = resource.trim_matches([' ', '\t', '\r', '\n']);
        if trimmed.is_empty() || trimmed.starts_with('/') {
            return Err("InvalidSkillResourcePath");
        }
        let segments: Vec<&str> = trimmed
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty()
            || segments
                .iter()
                .any(|segment| *segment == "." || *segment == "..")
        {
            return Err("InvalidSkillResourcePath");
        }
        opened = open_resource(&candidate.dir, &segments)?;
        &mut opened
    };
    let before = file.metadata().map_err(error_name)?;
    if !before.is_file() {
        return Err("InvalidSkillResource");
    }
    let observed = usize::try_from(before.len()).map_err(|_| "SkillFileLimitExceeded")?;
    let mut bytes = Vec::with_capacity(observed.min(FILE_LIMIT + 3));
    (&mut *file)
        .take((FILE_LIMIT + 3).min(observed) as u64)
        .read_to_end(&mut bytes)
        .map_err(error_name)?;
    // The whole file is UTF-8 or none of it is loaded: the part past the limit
    // too (`validateSkillResourceUtf8`), checked as it streams by.
    let utf8 = if observed > bytes.len() {
        let mut rest = Vec::new();
        file.read_to_end(&mut rest).map_err(error_name)?;
        let mut whole = bytes.clone();
        whole.extend_from_slice(&rest);
        std::str::from_utf8(&whole).is_ok()
    } else {
        std::str::from_utf8(&bytes).is_ok()
    };
    let after = file.metadata().map_err(error_name)?;
    if changed(&before, &after) {
        return Err("SkillResourceChanged");
    }
    if bytes.len() != observed.min(FILE_LIMIT + 3) {
        return Err("UnexpectedEndOfFile");
    }
    if !utf8 {
        return Err("BinarySkillResource");
    }
    let allowed = line_safe_prefix(&bytes, FILE_LIMIT);
    bytes.truncate(allowed);
    Ok(Read_ { bytes, observed })
}

/// Whether a file's size or modification time moved between two looks at it
/// (`final_stat.size != stat.size or final_stat.mtime != stat.mtime`).
fn changed(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    before.len() != after.len() || before.modified().ok() != after.modified().ok()
}

/// The skill's own `SKILL.md`, through the handle opened when it was
/// checked, read again from its start: its size and modification time the
/// same on either side of the read, and its name still the skill's
/// (`revalidatePrimaryIdentity`, `skill_invocation.zig:1316-1329`). Anything
/// else is `SkillResourceChanged`.
fn revalidate_primary(file: &File, skill: &Skill) -> Result<(), &'static str> {
    let before = file.metadata().map_err(error_name)?;
    let length = usize::try_from(before.len())
        .unwrap_or(usize::MAX)
        .min(contract::MAX_FRONTMATTER_BYTES + 1);
    let mut bytes = vec![0u8; length];
    let mut filled = 0;
    while filled < length {
        match read_at(file, &mut bytes[filled..], filled as u64) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error_name(error)),
        }
    }
    if filled != length {
        return Err("SkillResourceChanged");
    }
    let after = file.metadata().map_err(error_name)?;
    if changed(&before, &after) {
        return Err("SkillResourceChanged");
    }
    let fallback = skill
        .path
        .file_name()
        .map(|leaf| leaf.to_string_lossy().into_owned())
        .unwrap_or_default();
    match contract::resolve(&contract::parse(&bytes), &fallback) {
        Ok(metadata) if metadata.name == skill.name => Ok(()),
        _ => Err("SkillResourceChanged"),
    }
}

/// `buffer.len()` bytes of `file` from `offset`, wherever its cursor is.
#[cfg(unix)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buffer, offset)
}

#[cfg(not(unix))]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::io::{Seek, SeekFrom};
    let mut file = file;
    file.seek(SeekFrom::Start(offset))?;
    file.read(buffer)
}

/// The resource `segments` name, under the candidate's directory held open:
/// each component looked at without following it, then opened without
/// following it, so neither a link nor a component swapped in between is
/// read through (`openResource`'s `follow_symlinks = false`).
#[cfg(unix)]
fn open_resource(dir: &std::os::fd::OwnedFd, segments: &[&str]) -> Result<File, &'static str> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};
    use std::os::fd::AsFd;
    let mut owned: Option<std::os::fd::OwnedFd> = None;
    for (index, segment) in segments.iter().enumerate() {
        let last = index + 1 == segments.len();
        let parent = owned.as_ref().map_or(dir.as_fd(), |fd| fd.as_fd());
        let stat =
            rustix::fs::statat(parent, *segment, AtFlags::SYMLINK_NOFOLLOW).map_err(errno_name)?;
        match FileType::from_raw_mode(stat.st_mode) {
            FileType::Symlink => return Err("SymLinkLoop"),
            FileType::Directory if last => return Err("IsDir"),
            FileType::Directory => {}
            _ if !last => return Err("NotDir"),
            _ => {}
        }
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if last {
                OFlags::empty()
            } else {
                OFlags::DIRECTORY
            };
        let fd = rustix::fs::openat(parent, *segment, flags, Mode::empty()).map_err(errno_name)?;
        if last {
            return Ok(File::from(fd));
        }
        owned = Some(fd);
    }
    Err("InvalidSkillResourcePath")
}

#[cfg(not(unix))]
fn open_resource(dir: &PathBuf, segments: &[&str]) -> Result<File, &'static str> {
    let mut path = dir.clone();
    for (index, segment) in segments.iter().enumerate() {
        path.push(segment);
        let meta = fs::symlink_metadata(&path).map_err(error_name)?;
        let last = index + 1 == segments.len();
        if meta.file_type().is_symlink() {
            return Err("SymLinkLoop");
        }
        if !last && !meta.is_dir() {
            return Err("NotDir");
        }
        if last && meta.is_dir() {
            return Err("IsDir");
        }
    }
    discovery::open_no_follow(&path).map_err(error_name)
}

/// Upstream's error name for a failed `openat`/`statat`.
#[cfg(unix)]
fn errno_name(errno: rustix::io::Errno) -> &'static str {
    use rustix::io::Errno;
    match errno {
        Errno::NOENT => "FileNotFound",
        Errno::ACCESS | Errno::PERM => "AccessDenied",
        Errno::ISDIR => "IsDir",
        Errno::NOTDIR => "NotDir",
        Errno::LOOP => "SymLinkLoop",
        _ => "Unexpected",
    }
}

/// The longest UTF-8 prefix within `limit`, cut back to its last line break
/// when it is not the whole (`lineSafePrefixLength`,
/// `context_limits.zig:217-222`).
fn line_safe_prefix(bytes: &[u8], limit: usize) -> usize {
    let mut end = limit.min(bytes.len());
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    if end == bytes.len() {
        return end;
    }
    bytes[..end]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(end, |newline| newline + 1)
}

/// Upstream's error name for an I/O failure, which the refusal says.
fn error_name(error: io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        io::ErrorKind::IsADirectory => "IsDir",
        io::ErrorKind::NotADirectory => "NotDir",
        io::ErrorKind::UnexpectedEof => "UnexpectedEndOfFile",
        _ if error.raw_os_error() == Some(libc::ELOOP) => "SymLinkLoop",
        _ => "Unexpected",
    }
}

/// The marker a resource past [`FILE_LIMIT`] is refused with
/// (`writeSkillFileBlockedMarker`, `:1448-1462`).
fn file_blocked(resource: &str, observed: usize) -> String {
    let mut out = format!(
        "<context_limit name=\"skill_file_bytes\" action=\"blocked_remainder\" observed_bytes=\"{observed}\" effective_bytes=\"{FILE_LIMIT}\" source=\"compiled default\" resource=\""
    );
    write_scalar(&mut out, resource);
    out.push_str("\" override=\"--context-limit skill_file_bytes=BYTES|off\" />");
    out
}

/// `Skill "<name>" not found. …` (`formatMissingSkill`, `:1105-1110`).
fn missing(name: &str, budget: usize) -> String {
    bounded_identity(
        "Skill \"",
        name,
        "\" not found. Refresh available skills and retry with an advertised name.",
        "Skill not found. Refresh available skills and retry with an advertised name.",
        budget,
    )
}

fn exact_not_found(name: &str, location: &str, budget: usize) -> String {
    bounded_pair(
        ("Skill \"", name),
        ("\" was not found at advertised location \"", location),
        "\". Refresh available skills and retry with an advertised name and location.",
        "Skill was not found at the advertised location. Refresh available skills and retry with an advertised name and location.",
        budget,
    )
}

fn location_mismatch(name: &str, location: &str, budget: usize) -> String {
    bounded_pair(
        ("Skill \"", name),
        ("\" does not match the skill advertised at location \"", location),
        "\". Refresh available skills and retry with the advertised name and location.",
        "Skill name and location do not match one advertised skill. Refresh available skills and retry with the advertised name and location.",
        budget,
    )
}

/// One value inside a fixed sentence, cut to the budget with `...`
/// (`formatBoundedIdentityError`, `:1112-1137`).
fn bounded_identity(
    prefix: &str,
    value: &str,
    suffix: &str,
    fallback: &str,
    budget: usize,
) -> String {
    let fixed = prefix.len() + suffix.len();
    if fixed + 3 >= budget {
        return clip(fallback, budget);
    }
    let mut encoded = String::new();
    write_scalar(&mut encoded, value);
    let ellipsis = if encoded.len() > budget - fixed { 3 } else { 0 };
    let kept = encoded_prefix(&encoded, budget - fixed - ellipsis);
    let mut out = format!("{prefix}{}", &encoded[..kept]);
    if kept < encoded.len() {
        out.push_str("...");
    }
    out.push_str(suffix);
    clip(&out, budget)
}

/// Two values inside a fixed sentence (`formatBoundedIdentityPairError`,
/// `:1165-1202`).
fn bounded_pair(
    first: (&str, &str),
    second: (&str, &str),
    suffix: &str,
    fallback: &str,
    budget: usize,
) -> String {
    let fixed = first.0.len() + second.0.len() + suffix.len();
    if fixed + 6 >= budget {
        return clip(fallback, budget);
    }
    let mut one = String::new();
    write_scalar(&mut one, first.1);
    let mut two = String::new();
    write_scalar(&mut two, second.1);
    let available = budget - fixed - 6;
    let one_budget = one.len().min(available.min(contract::MAX_NAME_BYTES));
    let one_kept = encoded_prefix(&one, one_budget);
    let two_kept = encoded_prefix(&two, available - one_budget);
    let mut out = String::from(first.0);
    out.push_str(&one[..one_kept]);
    if one_kept < one.len() {
        out.push_str("...");
    }
    out.push_str(second.0);
    out.push_str(&two[..two_kept]);
    if two_kept < two.len() {
        out.push_str("...");
    }
    out.push_str(suffix);
    clip(&out, budget)
}

/// The longest prefix of an encoded value within `limit` that does not cut an
/// entity in half (`boundedEncodedPrefixLength`, `:1204-1210`).
fn encoded_prefix(encoded: &str, limit: usize) -> usize {
    let mut end = limit.min(encoded.len());
    while !encoded.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(amp) = encoded[..end].rfind('&') {
        if !encoded[amp..end].contains(';') {
            end = amp;
        }
    }
    end
}

/// A name more than one skill answers to, with every location it could mean
/// (`formatAmbiguousSkill`, `:1212-1257`).
fn ambiguous(skills: &[Skill], name: &str, budget: usize) -> String {
    let matching: Vec<&Skill> = skills.iter().filter(|skill| skill.name == name).collect();
    let mut out = String::from("Skill \"");
    write_scalar(&mut out, name);
    out.push_str("\" is ambiguous. Retry with the name and one advertised location: ");
    let suffix = |omitted: usize| {
        if omitted == 0 {
            ".".to_string()
        } else {
            format!(
                "; {omitted} additional advertised location{} omitted by the {budget}-byte tool-result limit. Refresh available skills and retry with an advertised name and location.",
                if omitted == 1 { "" } else { "s" }
            )
        }
    };
    let mut shown = 0usize;
    for skill in &matching {
        let mut choice = String::from("\"");
        write_scalar(&mut choice, &skill.path.to_string_lossy());
        choice.push('"');
        let separator = if shown > 0 { ", " } else { "" };
        let omitted = matching.len() - (shown + 1);
        if out.len() + separator.len() + choice.len() + suffix(omitted).len() > budget {
            break;
        }
        out.push_str(separator);
        out.push_str(&choice);
        shown += 1;
    }
    if shown == 0 {
        return clip(
            &format!(
                "Requested skill name is ambiguous; all {} advertised locations were omitted by the {budget}-byte tool-result limit. Refresh available skills and retry with an advertised name and location.",
                matching.len()
            ),
            budget,
        );
    }
    out.push_str(&suffix(matching.len() - shown));
    clip(&out, budget)
}

/// The `skill` tool's answer to `location` and `resource`
/// (`skill.zig` `prepareInput` and `loadSelected`, `vercel-labs/fx@c1db9191
/// src/tools/skills/skill.zig:129-183`): the location resolved against the
/// turn's catalog, the skill it names loaded whole -- and when the turn's
/// catalog does not have it, the skills discovered afresh.
pub fn tool_load(
    locations: Option<&Locations>,
    workspace: &Path,
    home: Option<&Path>,
    location: &str,
    resource: Option<&str>,
    budget: usize,
) -> Loaded {
    tool_call(
        locations,
        workspace,
        home,
        &Call {
            name: None,
            location: Some(location),
            resource,
            offset: 0,
        },
        budget,
    )
}

/// What one call of the `skill` tool names (`Input`, `skill.zig:14-26`): the
/// advertised `location`, or the legacy form's `name` with an optional
/// location and an `offset` to page from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call<'a> {
    pub name: Option<&'a str>,
    pub location: Option<&'a str>,
    pub resource: Option<&'a str>,
    pub offset: usize,
}

/// The `skill` tool's answer to a call (`prepareInput` and `loadSelected`,
/// `skill.zig:129-183`): a location resolved against the turn's catalog --
/// the skills discovered afresh when the catalog does not have it, or when
/// the call names only a skill -- and the skill it names loaded whole, or by
/// the legacy named form a chunk at a time.
pub fn tool_call(
    locations: Option<&Locations>,
    workspace: &Path,
    home: Option<&Path>,
    call: &Call,
    budget: usize,
) -> Loaded {
    let empty = Locations::default();
    let catalog = locations.unwrap_or(&empty);
    let path = match call.location.map(|location| catalog.resolve(location)) {
        Some(Ok(path)) => Some(path),
        Some(Err(error)) => {
            return Loaded::failure(format!(
                "skill failed: {}. Refresh available skills and retry with an exact advertised location.",
                error.name()
            ))
        }
        None => None,
    };
    let advertised = path.as_ref().is_some_and(|path| {
        catalog.skills.iter().any(|skill| skill.path == *path)
            || call
                .location
                .is_some_and(|location| location.starts_with("skill:"))
    });
    let found;
    let (skills, diagnostics) = if advertised {
        (&catalog.skills, &catalog.diagnostics)
    } else {
        found = discovery::discover(workspace, home);
        (&found.skills, &found.diagnostics)
    };
    match call.name {
        // The legacy named form, chunk by chunk (`loadByIdentity`).
        Some(name) => load_chunk(
            skills,
            diagnostics,
            name,
            path.as_deref(),
            call.resource,
            call.offset,
            Some(budget),
        ),
        None => match path {
            Some(path) => load_at(skills, diagnostics, &path, call.resource, budget),
            // The decoder refuses a call with neither.
            None => Loaded::failure("skill requires an advertised location".to_string()),
        },
    }
}

/// The skill at exactly `path`, loaded whole (`loadWholeByLocation`, `:640-658`).
fn load_at(
    skills: &[Skill],
    diagnostics: &[Diagnostic],
    path: &Path,
    resource: Option<&str>,
    budget: usize,
) -> Loaded {
    match skills.iter().find(|skill| skill.path == path) {
        Some(skill) => load_by_identity(
            skills,
            diagnostics,
            &skill.name,
            Some(path),
            resource,
            Some(budget),
        ),
        None => {
            let mut notice =
                (!diagnostics.is_empty()).then(|| super::diagnostic_summary(diagnostics));
            let bound = if notice.is_some() {
                budget.saturating_sub(DISCOVERY_MODEL_NOTICE.len())
            } else {
                budget
            };
            attach(
                Loaded::failure(exact_not_found("", &path.to_string_lossy(), bound)),
                &mut notice,
                Some(budget),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::discovery::Source;
    use super::*;

    thread_local! {
        static AFTER_VALIDATION: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
            const { std::cell::RefCell::new(None) };
    }

    /// Runs, once, what a case set to happen right after a load checked its
    /// candidate (upstream's `test_after_candidate_validation`,
    /// `skill_invocation.zig:140-150`, `:854`).
    pub(super) fn after_validation() {
        let hook = AFTER_VALIDATION.with(|slot| slot.borrow_mut().take());
        if let Some(mut hook) = hook {
            hook();
        }
    }

    /// `run`, with `hook` run between its load's check and its read.
    fn checked_then<T>(hook: impl FnMut() + 'static, run: impl FnOnce() -> T) -> T {
        AFTER_VALIDATION.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        let out = run();
        AFTER_VALIDATION.with(|slot| *slot.borrow_mut() = None);
        out
    }

    fn named(names: &[&str]) -> Vec<Skill> {
        names
            .iter()
            .map(|name| Skill {
                name: name.to_string(),
                description: String::new(),
                path: PathBuf::from(format!("/skills/{name}")),
                source: Source::Managed,
            })
            .collect()
    }

    fn picked(prompt: &str, skills: &[Skill]) -> Vec<String> {
        selections(prompt, skills)
            .into_iter()
            .map(|selection| match selection {
                Selection::Skill(index) => skills[index].name.clone(),
                Selection::Ambiguous(name) => format!("ambiguous:{name}"),
            })
            .collect()
    }

    #[test]
    fn a_dollar_word_names_a_skill_outside_quotes_code_and_paths() {
        let skills = named(&["demo", "demo-two", "review"]);
        assert_eq!(picked("$demo run it", &skills), ["demo"]);
        assert_eq!(picked("use $demo-two now", &skills), ["demo-two"]);
        assert_eq!(picked("$DEMO and $review", &skills), ["demo", "review"]);
        assert_eq!(
            picked("$demox is not a skill", &skills),
            Vec::<String>::new()
        );
        for unnamed in [
            "\"$demo\" quoted",
            "`$demo` in code",
            "@src/$demo in a path",
            "\\$demo escaped",
            "don't use $demo",
            "never $demo",
            "```\n$demo\n```",
        ] {
            assert_eq!(
                picked(unnamed, &skills),
                Vec::<String>::new(),
                "{unnamed:?}"
            );
        }
        assert_eq!(picked("$demo $demo", &skills), ["demo"]);
    }

    #[test]
    fn a_leading_slash_and_a_spoken_reference_name_a_skill_too() {
        let skills = named(&["release-notes", "demo"]);
        assert_eq!(picked("/release-notes for v2", &skills), ["release-notes"]);
        assert_eq!(
            picked("please use the release notes skill", &skills),
            ["release-notes"]
        );
        assert_eq!(picked("Run demo skill on this", &skills), ["demo"]);
        assert_eq!(
            picked("tell me about the demo skill", &skills),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_name_two_skills_share_is_ambiguous() {
        let mut skills = named(&["review"]);
        skills.push(Skill {
            path: PathBuf::from("/other/review"),
            ..skills[0].clone()
        });
        assert_eq!(picked("$review this", &skills), ["ambiguous:review"]);
    }

    fn workspace_skill(root: &Path, name: &str, body: &str) -> Skill {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n\n{body}"),
        )
        .unwrap();
        Skill {
            name: name.to_string(),
            description: "d".to_string(),
            path: dir,
            source: Source::Managed,
        }
    }

    #[test]
    fn an_explicit_block_carries_the_whole_file_as_upstreams() {
        let base = tempfile::tempdir().unwrap();
        let skills = vec![workspace_skill(
            base.path(),
            "demo",
            "# demo\n\nSay DEMO.\n",
        )];
        let block = explicit(&skills, &[], "$demo run it", &[]);
        assert_eq!(
            block.text,
            format!(
                "Explicitly invoked skill content for this query:\n\
Use every successfully loaded skill for this query. Report blocked or ambiguous requests.\n\
Follow each skill's complete instructions and required resources before substantive work.\n\
If a skill cannot be followed, state the blocker instead of silently substituting another workflow.\n\
<skill_content name=\"demo\" location=\"{}\" resource=\"SKILL.md\" complete=\"true\">\n\
---\nname: demo\ndescription: d\n---\n\n# demo\n\nSay DEMO.\n\n</skill_content>\n",
                skills[0].path.display()
            )
        );
        assert_eq!(
            block.load_notice,
            Some((
                false,
                "1 requested skill loaded\n\u{2514} Loaded skill demo".to_string()
            ))
        );
        assert_eq!(block.details, None);
        assert_eq!(
            explicit(&skills, &[], "nothing named", &[]),
            Explicit::default()
        );
    }

    #[test]
    fn a_binding_loads_its_exact_directory_once_and_a_gone_one_fails() {
        let base = tempfile::tempdir().unwrap();
        let skills = vec![workspace_skill(base.path(), "demo", "body\n")];
        let bound = Binding {
            name: "demo".to_string(),
            path: skills[0].path.clone(),
        };
        let block = explicit(&skills, &[], "$demo again", std::slice::from_ref(&bound));
        assert_eq!(block.text.matches("<skill_content").count(), 1);
        let gone = Binding {
            name: "gone".to_string(),
            path: base.path().join("gone"),
        };
        let block = explicit(&skills, &[], "x", &[gone]);
        let (warning, summary) = block.load_notice.unwrap();
        assert!(warning);
        assert_eq!(
            summary,
            "Requested skills \u{b7} 0 loaded \u{b7} 1 failed (ctrl+o for details)\n\u{2514} Could not load gone"
        );
        assert!(block
            .details
            .unwrap()
            .starts_with("Could not load gone: Skill \"gone\" was not found"));
    }

    #[test]
    fn the_tool_loads_by_location_and_refuses_a_stale_or_escaping_one() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("home/.xfx/skills");
        let skills = vec![workspace_skill(&root, "demo", "body\n")];
        fs::create_dir_all(skills[0].path.join("refs")).unwrap();
        fs::write(skills[0].path.join("refs/a.md"), "reference\n").unwrap();
        let catalog = super::super::catalog::build(&skills, &[], None);
        let location = catalog.locations.of(&skills[0]).unwrap();
        let workspace = base.path().join("work");
        fs::create_dir_all(&workspace).unwrap();

        let loaded = tool_load(
            Some(&catalog.locations),
            &workspace,
            None,
            &location,
            None,
            1 << 20,
        );
        assert!(loaded.ok && loaded.complete, "{loaded:?}");
        assert!(loaded.output.starts_with("<skill_content name=\"demo\""));
        let reference = tool_load(
            Some(&catalog.locations),
            &workspace,
            None,
            &location,
            Some("refs/a.md"),
            1 << 20,
        );
        assert!(reference
            .output
            .contains("resource=\"refs/a.md\" complete=\"true\">\nreference\n\n</skill_content>"));
        for escaping in ["../demo/SKILL.md", "/etc/passwd", "refs/../../x"] {
            let refused = tool_load(
                Some(&catalog.locations),
                &workspace,
                None,
                &location,
                Some(escaping),
                1 << 20,
            );
            assert!(!refused.ok, "{escaping}");
            assert_eq!(
                refused.output, "skill failed: InvalidSkillResourcePath",
                "{escaping}"
            );
        }
        let stale = format!("skill:{:016x}:0/demo", catalog.locations.namespace ^ 1);
        let refused = tool_load(
            Some(&catalog.locations),
            &workspace,
            None,
            &stale,
            None,
            1 << 20,
        );
        assert_eq!(
            refused.output,
            "skill failed: StaleSkillLocation. Refresh available skills and retry with an exact advertised location."
        );
    }

    /// A skill `workflow` with a `reference.md` beside it, under a fresh root.
    fn workflow(base: &Path) -> (Skill, PathBuf) {
        let root = base.join("home/.xfx/skills");
        let skill = workspace_skill(&root, "workflow", "ORIGINAL BODY\n");
        fs::write(skill.path.join("reference.md"), "REFERENCE BODY\n").unwrap();
        let path = skill.path.clone();
        (skill, path)
    }

    /// Rewrites `SKILL.md` in place -- the same file, a new name -- as
    /// upstream's cases do with `createFile` over it.
    fn rewrite(path: &Path, name: &str) {
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n\nCHANGED BODY\n"),
        )
        .unwrap();
    }

    #[test]
    fn a_resource_is_read_only_while_its_skill_is_still_the_one_that_was_checked() {
        // `skill references reject primary identity changes after candidate
        // validation` and `whole skill reads reject identity changes after
        // candidate validation` (`skill_invocation.zig:469-528`): the file
        // rewritten after the check, in both load modes.
        let base = tempfile::tempdir().unwrap();
        let (skill, path) = workflow(base.path());
        let skills = vec![skill];
        let load = |resource: Option<&str>| {
            load_by_identity(
                &skills,
                &[],
                "workflow",
                Some(&path),
                resource,
                Some(1 << 20),
            )
        };
        assert!(load(Some("reference.md")).ok);
        for resource in [Some("reference.md"), None] {
            let at = path.clone();
            let refused = checked_then(move || rewrite(&at, "renamed"), || load(resource));
            assert!(!refused.ok, "{resource:?}");
            assert_eq!(
                refused.output, "skill failed: SkillResourceChanged",
                "{resource:?}"
            );
            assert!(!refused.output.contains("CHANGED BODY"));
            rewrite(&path, "workflow");
        }
        let at = path.clone();
        let chunked = checked_then(
            move || rewrite(&at, "renamed"),
            || {
                load_chunk(
                    &skills,
                    &[],
                    "workflow",
                    Some(&path),
                    Some("reference.md"),
                    0,
                    None,
                )
            },
        );
        assert_eq!(chunked.output, "skill failed: SkillResourceChanged");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_replaced_by_a_link_is_refused_and_what_it_points_at_is_never_read() {
        let base = tempfile::tempdir().unwrap();
        let (skill, path) = workflow(base.path());
        let outside = base.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "OUTSIDE BODY\n").unwrap();
        fs::write(
            outside.join("SKILL.md"),
            "---\nname: workflow\ndescription: d\n---\n\nOUTSIDE BODY\n",
        )
        .unwrap();
        let skills = vec![skill];
        // After the check: the resource a link now (`openResource`'s
        // `follow_symlinks = false`).
        let (at, target) = (path.clone(), outside.join("secret.md"));
        let refused = checked_then(
            move || {
                fs::remove_file(at.join("reference.md")).unwrap();
                std::os::unix::fs::symlink(&target, at.join("reference.md")).unwrap();
            },
            || {
                load_by_identity(
                    &skills,
                    &[],
                    "workflow",
                    Some(&path),
                    Some("reference.md"),
                    None,
                )
            },
        );
        assert_eq!(refused.output, "skill failed: SymLinkLoop");
        // Before the load: `SKILL.md` a link out of a root with no authority,
        // so the advertised skill is not there to load
        // (`openPrimarySkillFile`'s `.sym_link` arm).
        fs::remove_file(path.join("SKILL.md")).unwrap();
        std::os::unix::fs::symlink(outside.join("SKILL.md"), path.join("SKILL.md")).unwrap();
        let gone = load_by_identity(&skills, &[], "workflow", Some(&path), None, None);
        assert!(!gone.ok);
        assert!(
            gone.output
                .starts_with("Skill \"workflow\" was not found at advertised location \""),
            "{}",
            gone.output
        );
        assert!(!gone.output.contains("OUTSIDE BODY"));
    }

    #[test]
    fn a_file_replaced_by_another_one_is_the_one_that_was_checked_or_nothing() {
        let base = tempfile::tempdir().unwrap();
        let (skill, path) = workflow(base.path());
        let skills = vec![skill];
        // Before the load: another `SKILL.md` renamed into place, under
        // another name -- the advertised skill is not this one.
        let staged = base.path().join("staged.md");
        fs::write(&staged, "---\nname: other\n---\nOTHER BODY\n").unwrap();
        fs::rename(&staged, path.join("SKILL.md")).unwrap();
        let mismatch = load_by_identity(&skills, &[], "workflow", Some(&path), None, None);
        assert!(!mismatch.ok);
        assert!(
            mismatch
                .output
                .contains("\" does not match the skill advertised at location \""),
            "{}",
            mismatch.output
        );
        assert!(!mismatch.output.contains("OTHER BODY"));
        rewrite(&path, "workflow");
        // After the check: the whole directory swapped for another, with the
        // same names in it. What is read is the directory that was checked
        // (`skill invocation reads validated candidate resources after path
        // replacement`, `skill_invocation.zig:1888-1985`).
        let replacement = base.path().join("replacement");
        fs::create_dir_all(&replacement).unwrap();
        fs::write(
            replacement.join("SKILL.md"),
            "---\nname: workflow\ndescription: d\n---\n\nREPLACEMENT BODY\n",
        )
        .unwrap();
        fs::write(replacement.join("reference.md"), "REPLACEMENT REFERENCE\n").unwrap();
        for resource in [None, Some("reference.md")] {
            let (at, from) = (path.clone(), replacement.clone());
            let aside = base.path().join("aside");
            let loaded = checked_then(
                move || {
                    fs::rename(&at, &aside).unwrap();
                    copy_dir(&from, &at);
                },
                || load_by_identity(&skills, &[], "workflow", Some(&path), resource, None),
            );
            assert!(loaded.ok, "{resource:?}: {loaded:?}");
            assert!(
                !loaded.output.contains("REPLACEMENT"),
                "{resource:?}: {}",
                loaded.output
            );
            assert!(
                loaded.output.contains("CHANGED BODY") || loaded.output.contains("REFERENCE BODY"),
                "{}",
                loaded.output
            );
            fs::remove_dir_all(&path).unwrap();
            fs::rename(base.path().join("aside"), &path).unwrap();
        }
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }

    #[test]
    fn the_named_form_checks_the_skill_again_at_every_offset() {
        let base = tempfile::tempdir().unwrap();
        let (skill, path) = workflow(base.path());
        let line = format!("{}\n", "x".repeat(99));
        fs::write(path.join("big.md"), line.repeat(300)).unwrap();
        let skills = vec![skill];
        let page = |offset: usize| {
            load_chunk(
                &skills,
                &[],
                "workflow",
                Some(&path),
                Some("big.md"),
                offset,
                None,
            )
        };
        let first = page(0);
        assert!(first.ok, "{first:?}");
        let next = CHUNK_LIMIT / 100 * 100;
        assert!(first.output.contains(&format!("next_offset=\"{next}\">")));
        // Rewritten while the next chunk is read: refused, nothing of it given.
        let at = path.clone();
        let changed = checked_then(move || rewrite(&at, "renamed"), || page(next));
        assert_eq!(changed.output, "skill failed: SkillResourceChanged");
        // Asked again with the skill still renamed: not the advertised one.
        let renamed = page(next);
        assert!(
            renamed
                .output
                .contains("\" does not match the skill advertised at location \""),
            "{}",
            renamed.output
        );
        // The skill back as it was: the chunk after, to the end, and no marker.
        rewrite(&path, "workflow");
        let rest = page(next);
        assert!(rest.ok, "{rest:?}");
        assert!(rest
            .output
            .contains(&format!("offset=\"{next}\" next_offset=\"30000\">")));
        assert!(!rest.output.contains("<context_limit"));
        assert_eq!(rest.notice, None);
    }
}
