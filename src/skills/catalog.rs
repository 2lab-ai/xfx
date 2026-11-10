//! The skills a model is told about: upstream's `<available_skills>` system
//! block, byte for byte, and the locations it hands out.
//!
//! `renderSkillPrompt` (`vercel-labs/fx@c1db9191
//! src/core/skills/skill_runtime.zig:3115-3322`): two sentences, the block's
//! opening tag, one `Root N: <dir>` line per directory a skill is in, one
//! `- <name>: <description> (location: skill:<namespace>:<root>/<leaf>)` line
//! per skill, and the closing tag. Every value is written as upstream's model
//! scalar ([`write_scalar`]), descriptions are cut at 1024 characters, and the
//! whole block keeps to a budget -- descriptions shortened first, then whole
//! entries omitted from the end with a count of them.
//!
//! **A location is a handle, not a path.** `skill:<namespace>:<index>/<leaf>`
//! names a root by its index in *this* block and the skill by its directory,
//! and the namespace is a hash of every advertised name and path, so a
//! location copied out of an older catalog is refused as stale rather than
//! resolved against roots that have moved (`Locations.resolve`,
//! `skill_contract.zig:85-108`).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::discovery::{Diagnostic, Scope, Skill};

/// The two sentences and the opening tag (`renderSkillPrompt`, `:3123-3126`).
const HEADER: &str = "Skills provide task instructions. Use named skills and clearly matching skills before substantive work.\n\
Read selected skills completely, including required references. Descriptions may be shortened; metadata is not loaded instructions.\n\
<available_skills>\n";

const FOOTER: &str = "</available_skills>\n";

/// The most characters of a description the block carries
/// (`skill_description_bytes`' compiled default, read as characters,
/// `:3175-3178`).
const DESCRIPTION_CHARACTERS: usize = 1024;

/// The block's budget when the model's context window is not known, in
/// characters (`SkillPromptBudget.resolve`, `:3061-3074`).
const BUDGET_CHARACTERS: usize = 8000;

/// The most a budget sized by the window may be, in bytes
/// (`context_limits.emergency_ceiling_bytes`,
/// `src/core/config/context_limits.zig:3`).
const CEILING_BYTES: u64 = 64 * 1024 * 1024;

/// What upstream calls the budget's source in its notices: xfx has no
/// `--context-limit` override, so the budget is always the compiled default's.
const BUDGET_SOURCE: &str = "compiled default";

/// How big the block may be (`SkillPromptBudget`, `skill_runtime.zig:3061-3083`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub limit: usize,
    /// Counted in characters rather than bytes.
    pub characters: bool,
}

impl Budget {
    /// Two percent of the model's context window, four bytes a token, under
    /// the ceiling; 8000 characters when the window is not known
    /// (`SkillPromptBudget.resolve`). A window of `200_000` is `16_000` bytes.
    pub fn for_window(window: Option<u64>) -> Self {
        match window.filter(|window| *window > 0) {
            Some(window) => Self {
                limit: usize::try_from(CEILING_BYTES.min((window * 2 / 100).max(1) * 4))
                    .unwrap_or(usize::MAX),
                characters: false,
            },
            None => Self {
                limit: BUDGET_CHARACTERS,
                characters: true,
            },
        }
    }

    /// What `text` costs against it.
    fn cost(self, text: &str) -> usize {
        if self.characters {
            text.chars().count()
        } else {
            text.len()
        }
    }

    /// The unit a notice names it in.
    fn unit(self) -> &'static str {
        if self.characters {
            "characters"
        } else {
            "bytes"
        }
    }
}

/// What a catalog build produced: the block, the notices about what it had to
/// leave out, and the locations it handed out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The system block, or empty when there is no skill to tell of.
    pub text: String,
    /// `[context] …` lines about shortened or omitted entries.
    pub notice: Option<String>,
    /// The discovery warning, when a candidate or a root was skipped.
    pub diagnostic_notice: Option<String>,
    pub locations: Locations,
}

/// What a location in the block resolves against (`Locations`,
/// `skill_contract.zig:79-108`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locations {
    pub namespace: u64,
    pub roots: Vec<PathBuf>,
    /// Every skill the catalog was built from, advertised or not.
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Why a location could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationError {
    Invalid,
    Stale,
}

impl LocationError {
    /// Upstream's error name, which the tool's refusal says.
    pub fn name(self) -> &'static str {
        match self {
            Self::Invalid => "InvalidSkillLocation",
            Self::Stale => "StaleSkillLocation",
        }
    }
}

impl Locations {
    /// The path `location` names: a `skill:` handle resolved against this
    /// catalog's roots, or anything else as the path it spells.
    pub fn resolve(&self, location: &str) -> Result<PathBuf, LocationError> {
        let Some(suffix) = location.strip_prefix("skill:") else {
            return Ok(PathBuf::from(location));
        };
        let bytes = suffix.as_bytes();
        if bytes.len() < 19 || bytes[16] != b':' {
            return Err(LocationError::Invalid);
        }
        let namespace =
            u64::from_str_radix(&suffix[..16], 16).map_err(|_| LocationError::Invalid)?;
        if namespace != self.namespace {
            return Err(LocationError::Stale);
        }
        let rest = &suffix[17..];
        let slash = rest.find('/').ok_or(LocationError::Invalid)?;
        let digits = &rest[..slash];
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(LocationError::Invalid);
        }
        let index: usize = digits.parse().map_err(|_| LocationError::Invalid)?;
        let root = self.roots.get(index).ok_or(LocationError::Invalid)?;
        let leaf = percent_decode(&rest[slash + 1..]).ok_or(LocationError::Invalid)?;
        if leaf.is_empty() || leaf == "." || leaf == ".." || leaf.contains(['/', '\\', '\0']) {
            return Err(LocationError::Invalid);
        }
        Ok(root.join(leaf))
    }

    /// The location the block gives `skill`, when it advertised it.
    pub fn of(&self, skill: &Skill) -> Option<String> {
        let root = skill.path.parent()?;
        let index = self.roots.iter().position(|kept| kept == root)?;
        let leaf = skill.path.file_name()?.to_str()?;
        Some(format!(
            "skill:{:016x}:{index}/{}",
            self.namespace,
            percent_encode(leaf)
        ))
    }
}

/// The catalog for `skills`, sized by the context window of the model the
/// turn talks to when one is known, and the warning about what discovery
/// skipped (`buildSkillPrompt`, `:3023-3059`; the window is the request's,
/// `prepareSkillCatalog`, `orchestrator.zig:5440-5458`, `:5617`).
pub fn build(skills: &[Skill], diagnostics: &[Diagnostic], window: Option<u64>) -> Catalog {
    // A name or a path a model cannot be shown as it is -- not UTF-8, or with
    // a NUL in it -- would be advertised as something else, so it is not
    // advertised at all (`modelProjectionPreservesText`).
    let visible: Vec<&Skill> = skills
        .iter()
        .filter(|skill| model_safe(&skill.name) && skill.path.to_str().is_some_and(model_safe))
        .collect();
    let namespace = namespace(visible.iter().copied());
    let mut catalog = render(&visible, namespace, Budget::for_window(window));
    catalog.locations.skills = skills.to_vec();
    catalog.locations.diagnostics = diagnostics.to_vec();
    if visible.len() < skills.len() {
        let withheld = format!(
            "{}[context] {} skill identities withheld because they cannot be safely represented to the model.\n",
            catalog.notice.as_deref().unwrap_or_default(),
            skills.len() - visible.len()
        );
        catalog.notice = Some(withheld);
    }
    if !diagnostics.is_empty() {
        let candidates = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.scope == Scope::Candidate)
            .count();
        let roots = diagnostics.len() - candidates;
        catalog.text = format!(
            "<skill_discovery_warning skipped_candidate_count=\"{candidates}\" incomplete_root_count=\"{roots}\" missing_from_incomplete_roots=\"{}\" />\n{}",
            if roots > 0 { "unknown" } else { "0" },
            catalog.text
        );
        catalog.diagnostic_notice = Some(super::diagnostic_summary(diagnostics));
    }
    catalog
}

/// The hash a location's namespace is: SHA-256 over upstream's label and every
/// advertised name and path, each preceded by its length, little-endian, and
/// the first eight bytes of the digest read little-endian
/// (`catalog_namespace`, `:3099-3113`).
///
/// Upstream's label, `fx-skill-locations`, is kept: the namespace is a value
/// the model copies back, and the same skills under the same paths get the
/// same handle in either product.
pub fn namespace<'a>(skills: impl IntoIterator<Item = &'a Skill>) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"fx-skill-locations");
    for skill in skills {
        let path = skill.path.to_string_lossy();
        for value in [skill.name.as_bytes(), path.as_bytes()] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value);
        }
    }
    let digest = hash.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first)
}

/// One entry, in its three parts: what is always written, the description
/// the budget may shorten, and the location.
struct Entry {
    prefix: String,
    description: String,
    suffix: String,
    /// How many roots exist once this entry's has been listed.
    root_count: usize,
    /// How much of the description the budget kept.
    kept: usize,
    limited: bool,
}

fn render(skills: &[&Skill], namespace: u64, budget: Budget) -> Catalog {
    if skills.is_empty() {
        return Catalog::default();
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut root_lines: Vec<String> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    for skill in skills {
        let root = skill.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let index = match roots.iter().position(|kept| *kept == root) {
            Some(index) => index,
            None => {
                let mut line = format!("Root {}: ", roots.len());
                write_scalar(&mut line, &root.to_string_lossy());
                line.push('\n');
                roots.push(root);
                root_lines.push(line);
                roots.len() - 1
            }
        };
        let mut prefix = String::from("- ");
        write_scalar(&mut prefix, &skill.name);
        prefix.push_str(": ");
        let cut = character_prefix(&skill.description, DESCRIPTION_CHARACTERS);
        let mut description = String::new();
        write_scalar(&mut description, &skill.description[..cut]);
        let leaf = skill
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let suffix = format!(
            " (location: skill:{namespace:016x}:{index}/{})\n",
            percent_encode(&leaf)
        );
        entries.push(Entry {
            prefix,
            description,
            suffix,
            root_count: roots.len(),
            kept: 0,
            limited: cut < skill.description.len(),
        });
    }

    let cost = |text: &str| budget.cost(text);
    let limit = budget.limit;
    let base = cost(HEADER) + cost(FOOTER);
    let mut minimum = base
        + root_lines.iter().map(|line| cost(line)).sum::<usize>()
        + entries
            .iter()
            .map(|entry| cost(&entry.prefix) + cost(&entry.suffix))
            .sum::<usize>();
    let mut retained = entries.len();
    let mut retained_roots = roots.len();
    let mut marker = String::new();
    if minimum > limit {
        retained = 0;
        retained_roots = 0;
        minimum = base;
        let mut candidate = base;
        let mut candidate_roots = 0usize;
        for (index, entry) in entries.iter().enumerate() {
            for line in &root_lines[candidate_roots..entry.root_count] {
                candidate += cost(line);
            }
            candidate_roots = entry.root_count;
            candidate += cost(&entry.prefix) + cost(&entry.suffix);
            let omitted = format!("Omitted skills: {}.\n", entries.len() - index - 1);
            if candidate + cost(&omitted) > limit {
                break;
            }
            retained = index + 1;
            retained_roots = candidate_roots;
            minimum = candidate;
        }
        marker = format!("Omitted skills: {}.\n", entries.len() - retained);
        minimum += cost(&marker);
    }

    // The descriptions get what is left, one scalar per entry per round, so a
    // tight budget shortens every description a little rather than dropping
    // the last ones whole.
    let mut available = limit.saturating_sub(minimum);
    let mut progress = true;
    while progress && available > 0 {
        progress = false;
        for entry in entries[..retained].iter_mut() {
            let rest = &entry.description[entry.kept..];
            if rest.is_empty() {
                continue;
            }
            let length = encoded_scalar_length(rest);
            let spent = cost(&rest[..length]);
            if spent > available {
                continue;
            }
            entry.kept += length;
            available -= spent;
            progress = true;
        }
    }

    let mut text = String::new();
    if minimum <= limit {
        text.push_str(HEADER);
        for line in &root_lines[..retained_roots] {
            text.push_str(line);
        }
        for entry in &entries[..retained] {
            text.push_str(&entry.prefix);
            text.push_str(&entry.description[..entry.kept]);
            text.push_str(&entry.suffix);
        }
        text.push_str(&marker);
        text.push_str(FOOTER);
    } else {
        retained = 0;
        retained_roots = 0;
    }

    let mut notices = String::new();
    let limited = entries[..retained]
        .iter()
        .filter(|entry| entry.limited)
        .count();
    if limited > 0 {
        notices.push_str(&format!(
            "[context] skill descriptions shortened: {limited}; source={BUDGET_SOURCE}\n"
        ));
    }
    let shortened = entries[..retained]
        .iter()
        .filter(|entry| entry.kept < entry.description.len())
        .count();
    if shortened > 0 {
        notices.push_str(&format!(
            "[context] skill catalog shortened {shortened} descriptions: effective={limit} {} source={BUDGET_SOURCE}\n",
            budget.unit()
        ));
    }
    if retained < skills.len() {
        let omitted = &skills[retained..];
        let shown: Vec<String> = omitted
            .iter()
            .take(OMITTED_NAMES)
            .map(|skill| bounded_name(&skill.name))
            .collect();
        let more = if omitted.len() > OMITTED_NAMES {
            format!(", +{} more", omitted.len() - OMITTED_NAMES)
        } else {
            String::new()
        };
        notices.push_str(&format!(
            "[context] skill catalog omitted {} entries ({}{more}): effective={limit} {} source={BUDGET_SOURCE}\n",
            omitted.len(),
            shown.join(", "),
            budget.unit()
        ));
    }
    Catalog {
        text,
        notice: (!notices.is_empty()).then_some(notices),
        diagnostic_notice: None,
        locations: Locations {
            namespace,
            roots: roots[..retained_roots].to_vec(),
            skills: Vec::new(),
            diagnostics: Vec::new(),
        },
    }
}

/// How many omitted names the notice spells out (`catalog_notice_name_count`).
const OMITTED_NAMES: usize = 8;

/// A name in a notice, encoded and cut at the name cap with `...`
/// (`writeBoundedSkillName`).
fn bounded_name(name: &str) -> String {
    let mut encoded = String::new();
    write_scalar(&mut encoded, name);
    let limit = super::contract::MAX_NAME_BYTES;
    if encoded.len() <= limit {
        return encoded;
    }
    let mut cut = limit;
    while !encoded.is_char_boundary(cut) {
        cut -= 1;
    }
    if let Some(amp) = encoded[..cut].rfind('&') {
        if !encoded[amp..cut].contains(';') {
            cut = amp;
        }
    }
    format!("{}...", &encoded[..cut])
}

/// The bytes of the first `characters` characters of `text`
/// (`skillDescriptionPrefix`, `:3324-3332`).
fn character_prefix(text: &str, characters: usize) -> usize {
    text.char_indices()
        .nth(characters)
        .map_or(text.len(), |(offset, _)| offset)
}

/// One scalar of encoded text: an entity whole, or one character
/// (`encodedScalarLength`, `:3334-3339`).
fn encoded_scalar_length(text: &str) -> usize {
    if text.starts_with('&') {
        if let Some(end) = text.find(';') {
            return end + 1;
        }
    }
    text.chars().next().map_or(0, char::len_utf8)
}

/// Whether a model is shown `text` as it is: no NUL (the string is UTF-8
/// already) (`isModelSafeText`, `text_utils.zig:17-20`).
pub fn model_safe(text: &str) -> bool {
    !text.contains('\0')
}

/// `value` as upstream writes a scalar into model context: `& < > "` and
/// every C0 control and DEL as entities, and the three Unicode line breaks
/// that are not C0 as hex entities (`writeScalar`,
/// `vercel-labs/fx@c1db9191 src/core/shared/model_context_encoding.zig:4-34`).
pub fn write_scalar(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{85}' => out.push_str("&#x85;"),
            '\u{2028}' => out.push_str("&#x2028;"),
            '\u{2029}' => out.push_str("&#x2029;"),
            control if (control as u32) < 0x20 || control == '\u{7f}' => {
                out.push_str(&format!("&#x{:02x};", control as u32));
            }
            other => out.push(other),
        }
    }
}

/// `leaf` percent-encoded as a URI component: everything but letters, digits
/// and `- . _ ~` (`std.Uri.Component.formatEscaped`, `isUnreserved`).
fn percent_encode(leaf: &str) -> String {
    let mut out = String::with_capacity(leaf.len());
    for byte in leaf.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A percent-encoded leaf decoded; `None` for a `%` that is not followed by two
/// hex digits, or bytes that do not decode to UTF-8.
fn percent_decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = bytes.get(index + 1..index + 3)?;
            if !digits.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            let text = std::str::from_utf8(digits).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::super::discovery::Source;
    use super::*;

    fn skill(name: &str, description: &str, path: &str, source: Source) -> Skill {
        Skill {
            name: name.to_string(),
            description: description.to_string(),
            path: PathBuf::from(path),
            source,
        }
    }

    /// The capture's run directory, as upstream's request named it. The
    /// namespace (`skill:<hash>`) is a hash of the roots under it, so the
    /// block is upstream's only for these bytes: a fixture that keeps a
    /// local path's shape on purpose.
    const BASE: &str = "/private/tmp/claude-501/fxcap.BElQiS"; // local-path-ok: upstream's bytes

    /// The capture's own skills and paths (`research/media-ext/cap/
    /// fx-skills-40x120-bodies/001-completion.json`), so the block is checked
    /// against the bytes upstream sent, namespace included.
    fn captured() -> Vec<Skill> {
        let base = BASE;
        vec![
            skill(
                "demo",
                "Demo workspace skill for the capture",
                &format!("{base}/work/.fx/skills/demo"),
                Source::WorkspaceXfx,
            ),
            skill(
                "release-notes",
                "Write release notes from the git log",
                &format!("{base}/work/skills/release-notes"),
                Source::WorkspaceShared,
            ),
            skill(
                "foo",
                "A skill installed from a local directory",
                &format!("{base}/home/.fx/skills/foo"),
                Source::Managed,
            ),
        ]
    }

    #[test]
    fn the_block_is_upstreams_byte_for_byte() {
        let catalog = build(&captured(), &[], None);
        let base = BASE;
        assert_eq!(
            catalog.text,
            format!("Skills provide task instructions. Use named skills and clearly matching skills before substantive work.\n\
Read selected skills completely, including required references. Descriptions may be shortened; metadata is not loaded instructions.\n\
<available_skills>\n\
Root 0: {base}/work/.fx/skills\n\
Root 1: {base}/work/skills\n\
Root 2: {base}/home/.fx/skills\n\
- demo: Demo workspace skill for the capture (location: skill:779085e8a24f238b:0/demo)\n\
- release-notes: Write release notes from the git log (location: skill:779085e8a24f238b:1/release-notes)\n\
- foo: A skill installed from a local directory (location: skill:779085e8a24f238b:2/foo)\n\
</available_skills>\n")
        );
        assert_eq!(catalog.notice, None);
        assert_eq!(catalog.diagnostic_notice, None);
    }

    #[test]
    fn no_skill_is_no_block() {
        assert_eq!(build(&[], &[], None).text, "");
    }

    #[test]
    fn a_location_resolves_against_its_catalog_and_a_stale_one_is_refused() {
        // Upstream's cases (`skill locations preserve exact roots and reject
        // stale namespaces`, `skill_contract.zig`).
        let locations = Locations {
            namespace: 19,
            roots: vec![PathBuf::from("/first"), PathBuf::from("/second")],
            ..Locations::default()
        };
        assert_eq!(
            locations.resolve("skill:0000000000000013:1/review"),
            Ok(PathBuf::from("/second/review"))
        );
        assert_eq!(
            locations.resolve("skill:0000000000000012:1/review"),
            Err(LocationError::Stale)
        );
        for invalid in [
            "skill:0000000000000013:2/review",
            "skill:0000000000000013:0/../review",
            "skill:0000000000000013:0/..",
            "skill:0000000000000013:0/%2e%2e",
            "skill:0000000000000013:0/review%2fother",
            "skill:0000000000000013:0/bad%zz",
        ] {
            assert_eq!(
                locations.resolve(invalid),
                Err(LocationError::Invalid),
                "{invalid}"
            );
        }
        assert_eq!(
            locations.resolve("skill:0000000000000013:0/review%26more"),
            Ok(PathBuf::from("/first/review&more"))
        );
        assert_eq!(
            locations.resolve("/plain/path"),
            Ok(PathBuf::from("/plain/path"))
        );
    }

    #[test]
    fn scalars_are_encoded_as_upstreams() {
        let mut out = String::new();
        write_scalar(&mut out, "a&b<c>d\"e\tf\u{7f}g\u{2028}h'");
        assert_eq!(out, "a&amp;b&lt;c&gt;d&quot;e&#x09;f&#x7f;g&#x2028;h'");
        assert_eq!(percent_encode("a b&c~"), "a%20b%26c~");
    }

    #[test]
    fn a_long_description_is_cut_at_1024_characters_and_said_so() {
        let long = "é".repeat(1100);
        let catalog = build(&[skill("x", &long, "/r/x", Source::Managed)], &[], None);
        let line = catalog
            .text
            .lines()
            .find(|line| line.starts_with("- x: "))
            .unwrap();
        assert_eq!(line.matches('é').count(), 1024);
        assert_eq!(
            catalog.notice.as_deref(),
            Some("[context] skill descriptions shortened: 1; source=compiled default\n")
        );
    }

    #[test]
    fn a_catalog_past_the_budget_omits_from_the_end_and_counts_what_it_left() {
        let skills: Vec<Skill> = (0..200)
            .map(|index| {
                skill(
                    &format!("skill-{index:03}"),
                    "a description",
                    &format!("/root/skill-{index:03}"),
                    Source::Managed,
                )
            })
            .collect();
        let catalog = build(&skills, &[], None);
        assert!(catalog.text.chars().count() <= BUDGET_CHARACTERS);
        let marker = catalog
            .text
            .lines()
            .find(|line| line.starts_with("Omitted skills: "))
            .expect("a marker");
        let listed = catalog
            .text
            .lines()
            .filter(|line| line.starts_with("- "))
            .count();
        assert_eq!(marker, format!("Omitted skills: {}.", 200 - listed));
        assert!(catalog.notice.unwrap().contains("skill catalog omitted"));
    }

    #[test]
    fn the_budget_is_two_percent_of_the_models_window_in_bytes_and_8000_characters_without_one() {
        // `skill catalog uses model capacity and preserves explicit byte
        // overrides` (`skill_runtime.zig:4479-4500`).
        assert_eq!(
            Budget::for_window(None),
            Budget {
                limit: 8000,
                characters: true
            }
        );
        assert_eq!(Budget::for_window(Some(0)), Budget::for_window(None));
        assert_eq!(
            Budget::for_window(Some(200_000)),
            Budget {
                limit: 16_000,
                characters: false
            }
        );
        // At least one token, and never past the ceiling.
        assert_eq!(Budget::for_window(Some(10)).limit, 4);
        assert_eq!(
            Budget::for_window(Some(u64::MAX / 4)).limit,
            64 * 1024 * 1024
        );
        let skills: Vec<Skill> = (0..16)
            .map(|index| {
                skill(
                    &format!("entry-{index}"),
                    &"useful instructions ".repeat(80),
                    &format!("/r/entry-{index}"),
                    Source::Managed,
                )
            })
            .collect();
        let unknown = build(&skills, &[], None);
        let large = build(&skills, &[], Some(200_000));
        assert!(unknown.text.chars().count() <= 8000);
        assert!(large.text.len() > unknown.text.len());
        assert!(large.text.len() <= 16_000);
        assert!(
            large.notice.as_deref().is_some_and(
                |notice| notice.contains("effective=16000 bytes source=compiled default")
            ),
            "{:?}",
            large.notice
        );
        assert!(unknown.notice.as_deref().is_some_and(
            |notice| notice.contains("effective=8000 characters source=compiled default")
        ));
    }
}
