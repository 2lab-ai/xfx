//! The source pins: where the product makes a directory, read off the source
//! with its test code cut away.
//!
//! What this checks, and no more: every product `.rs` file under `src/` --
//! one Cargo package, so all of xfx's code -- is read without comments and
//! without the contents of its string and character literals, and without
//! every item or statement under a `#[cfg(test)]` attribute, which is cut to
//! its own end (an inline test module to its closing brace), so code after a
//! test module is still read. A file that is a `#[cfg(test)]` module, or a
//! module declared inside one, is test code and skipped. In what is left, the
//! spellings in [`BANNED`] appear only on the lines [`ALLOWED`] names. A call
//! under another name -- an import alias, a wrapper, a crate this does not
//! know -- is not seen; the behavioural tests in `super` are the check on
//! what the writers actually do.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The spellings that make a directory.
const BANNED: &[&str] = &[
    "create_dir_all(",
    "create_dir(",
    "DirBuilder::new(",
    "mkdir(",
    "mkdirat(",
];

/// Every product line allowed one of them, by file, and why.
const ALLOWED: &[(&str, &str)] = &[
    // The helper itself: one level, `0700` from its creation.
    (
        "src/profile_home.rs",
        "fs::DirBuilder::new().mode(DIR_MODE).create(path)",
    ),
    ("src/profile_home.rs", "fs::create_dir(path)"),
    // A session's directory and its `images/`, below `sessions/`, which is
    // checked private; each chmodded `0700` after it is made.
    ("src/session/store.rs", "match fs::create_dir(&dir) {"),
    ("src/session/store.rs", "match fs::create_dir(&folder) {"),
    // The store's own one-level private make, and its non-unix twin.
    (
        "src/session/store.rs",
        "fs::DirBuilder::new().mode(0o700).create(path)",
    ),
    ("src/session/store.rs", "fs::create_dir(path)"),
    // `$TMPDIR`, not the profile.
    (
        "src/clipboard.rs",
        "match std::fs::DirBuilder::new().mode(0o700).create(&dir) {",
    ),
    // The model's write tool, in the workspace (a `.xfx` there is refused).
    (
        "src/tools/mutate.rs",
        "match rustix::fs::mkdirat(current, component.as_os_str(), new_directory_mode()) {",
    ),
    (
        "src/tools/mutate.rs",
        "return match rustix::fs::mkdirat(parent, located.name.as_os_str(), new_directory_mode())",
    ),
];

/// `text` as the product reads it, and the names of the modules it declares
/// test-only (`#[cfg(test)] mod NAME;`), whose files are test code.
///
/// Comments are taken out; a string or character literal is left empty
/// (`""`, `' '`); every item or statement under a `#[cfg(test)]` attribute is
/// replaced by its newlines alone, so lines keep their numbers.
pub(super) fn product(text: &str) -> (String, Vec<String>) {
    let clean = strip_comments_and_literals(text);
    cut_test_items(&clean)
}

/// `text` without comments and with every literal emptied; newlines kept.
fn strip_comments_and_literals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        if c == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            let mut depth = 0;
            while at < chars.len() {
                if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
                    depth += 1;
                    at += 2;
                } else if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[at] == '\n' {
                        out.push('\n');
                    }
                    at += 1;
                }
            }
            continue;
        }
        // A raw string, `r#"…"#` or `br"…"`, its prefix not part of a name.
        if (c == 'r' || (c == 'b' && next == Some('r'))) && (at == 0 || !ident(chars[at - 1])) {
            let mut probe = at + if c == 'b' { 2 } else { 1 };
            let mut hashes = 0;
            while chars.get(probe) == Some(&'#') {
                hashes += 1;
                probe += 1;
            }
            if chars.get(probe) == Some(&'"') {
                probe += 1;
                loop {
                    match chars.get(probe) {
                        None => break,
                        Some('"') if (1..=hashes).all(|n| chars.get(probe + n) == Some(&'#')) => {
                            probe += 1 + hashes;
                            break;
                        }
                        Some('\n') => out.push('\n'),
                        Some(_) => {}
                    }
                    probe += 1;
                }
                out.push_str("\"\"");
                at = probe;
                continue;
            }
        }
        if c == '"' {
            at += 1;
            while at < chars.len() && chars[at] != '"' {
                if chars[at] == '\\' {
                    at += 1;
                }
                if chars.get(at) == Some(&'\n') {
                    out.push('\n');
                }
                at += 1;
            }
            out.push_str("\"\"");
            at += 1;
            continue;
        }
        if c == '\'' {
            // A character literal -- `'x'`, `'\n'`, `'\u{2500}'` -- and not a
            // lifetime or a label, which has no closing quote after one char.
            if next == Some('\\') {
                // Past the backslash and the character it escapes.
                let mut probe = at + 3;
                while probe < chars.len() && chars[probe] != '\'' {
                    probe += 1;
                }
                out.push_str("' '");
                at = probe + 1;
                continue;
            }
            if chars.get(at + 2) == Some(&'\'') {
                out.push_str("' '");
                at += 3;
                continue;
            }
        }
        out.push(c);
        at += 1;
    }
    out
}

/// `clean` (no comments, no literal contents) with every item or statement
/// under a test-only attribute -- `#[cfg(test)]`, `#[cfg(all(test, …))]` --
/// replaced by its newlines, and the test-only modules declared.
///
/// An item ends at its first `;`, or at the brace closing its first `{`; a
/// field, an argument or an arm -- anything that does not start with an item's
/// keyword -- ends at its first `,` as well. Each is looked for outside any
/// parenthesis or bracket, and a closer that would close the enclosing block
/// ends it too.
fn cut_test_items(clean: &str) -> (String, Vec<String>) {
    const ITEMS: &[&str] = &[
        "fn",
        "pub",
        "impl",
        "mod",
        "use",
        "struct",
        "enum",
        "const",
        "static",
        "type",
        "trait",
        "unsafe",
        "async",
        "extern",
        "let",
        "macro_rules",
        "thread_local",
    ];
    let chars: Vec<char> = clean.chars().collect();
    let mut out = String::with_capacity(clean.len());
    let mut declared = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let test_only =
            starts_with(&chars, at, "#[cfg(test)]") || starts_with(&chars, at, "#[cfg(all(test");
        if !test_only {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let start = at;
        at = matching(&chars, at + 1, '[', ']');
        // Any further attributes on the same item.
        loop {
            at = skip_space(&chars, at);
            if chars.get(at) == Some(&'#') && chars.get(at + 1) == Some(&'[') {
                at = matching(&chars, at + 1, '[', ']');
            } else {
                break;
            }
        }
        let item_start = at;
        let word: String = chars[item_start..]
            .iter()
            .take_while(|c| c.is_alphanumeric() || **c == '_')
            .collect();
        let item_like = ITEMS.contains(&word.as_str());
        let mut depth = 0i32;
        let mut end = chars.len();
        let mut probe = at;
        while probe < chars.len() {
            match chars[probe] {
                '(' | '[' => depth += 1,
                ')' | ']' | '}' if depth == 0 => {
                    end = probe;
                    break;
                }
                ')' | ']' => depth -= 1,
                ';' if depth == 0 => {
                    end = probe + 1;
                    break;
                }
                ',' if depth == 0 && !item_like => {
                    end = probe + 1;
                    break;
                }
                '{' if depth == 0 => {
                    end = matching(&chars, probe, '{', '}');
                    break;
                }
                _ => {}
            }
            probe += 1;
        }
        let item: String = chars[item_start..end].iter().collect();
        if let Some(name) = declared_module(&item) {
            declared.push(name);
        }
        out.extend(chars[start..end].iter().filter(|c| **c == '\n'));
        at = end;
    }
    (out, declared)
}

/// The name in a `mod NAME;` item (of any visibility), if `item` is one.
fn declared_module(item: &str) -> Option<String> {
    let item = item.trim().strip_suffix(';')?.trim_end();
    let words: Vec<&str> = item.split_whitespace().collect();
    match words.as_slice() {
        [.., "mod", name] if name.chars().all(|c| c.is_alphanumeric() || c == '_') => {
            Some((*name).to_string())
        }
        _ => None,
    }
}

/// Every `mod NAME;` the file declares, test-only or not.
fn declared_modules(clean: &str) -> Vec<String> {
    clean
        .split(';')
        .filter_map(|piece| {
            let tail = piece.rsplit(['{', '}']).next()?;
            declared_module(&format!("{tail};"))
        })
        .collect()
}

fn starts_with(chars: &[char], at: usize, text: &str) -> bool {
    text.chars()
        .enumerate()
        .all(|(n, c)| chars.get(at + n) == Some(&c))
}

fn skip_space(chars: &[char], mut at: usize) -> usize {
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    at
}

/// The index just past the `close` matching the `open` at `at`.
fn matching(chars: &[char], at: usize, open: char, close: char) -> usize {
    let mut depth = 0;
    let mut probe = at;
    while probe < chars.len() {
        if chars[probe] == open {
            depth += 1;
        } else if chars[probe] == close {
            depth -= 1;
            if depth == 0 {
                return probe + 1;
            }
        }
        probe += 1;
    }
    chars.len()
}

/// The directory `file`'s declared modules live in.
fn module_dir(file: &Path) -> PathBuf {
    let parent = file.parent().expect("a parent").to_path_buf();
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => parent,
        _ => parent.join(file.file_stem().expect("a stem")),
    }
}

fn module_files(file: &Path, name: &str) -> [PathBuf; 2] {
    let dir = module_dir(file);
    [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ]
}

/// Every product file under `src/`, as [`product`] reads it, keyed by its
/// path relative to the package.
fn product_tree() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut pending = vec![root.join("src")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("a listing") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    let mut read = BTreeMap::new();
    let mut tests = std::collections::BTreeSet::new();
    let mut children: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("a source");
        let (code, test_modules) = product(&text);
        for name in &test_modules {
            tests.extend(module_files(file, name));
        }
        let all = declared_modules(&strip_comments_and_literals(&text));
        children.insert(
            file.clone(),
            all.iter()
                .flat_map(|name| module_files(file, name))
                .collect(),
        );
        read.insert(file.clone(), code);
    }
    // A module declared inside test code is test code too.
    let mut frontier: Vec<PathBuf> = tests.iter().cloned().collect();
    while let Some(file) = frontier.pop() {
        for child in children.get(&file).into_iter().flatten() {
            if tests.insert(child.clone()) {
                frontier.push(child.clone());
            }
        }
    }
    read.into_iter()
        .filter(|(file, _)| !tests.contains(file))
        .map(|(file, code)| {
            let relative = file.strip_prefix(root).expect("in the package");
            (relative.display().to_string(), code)
        })
        .collect()
}

#[test]
fn a_test_module_is_cut_to_its_own_end_and_the_code_after_it_is_read() {
    let fixture = r##"
fn before() { std::fs::create_dir_all("a").ok(); }
#[cfg(test)]
mod tests {
    fn braces_in_text() { let _ = "}"; let _ = '}'; let _ = r#"} {"#; /* } */ // }
        std::fs::create_dir_all("t").ok(); }
    mod nested { fn inner() {} }
}
fn after<'a>(x: &'a str) -> &'a str { let _ = std::fs::DirBuilder::new(); x }
#[cfg(test)]
pub(crate) mod declared;
#[cfg(test)]
#[allow(dead_code)]
fn helper() { std::fs::create_dir("h").ok(); }
fn last() { let _ = "create_dir_all( in a string"; let _ = '\''; }
struct Seam {
    #[cfg(test)]
    hand: Option<fn(&mut u8)>,
    kept: u8,
}
fn call() { go(1, #[cfg(test)] std::fs::create_dir("arg"), 2); std::fs::create_dir_all("x"); }
#[cfg(all(test, unix))]
mod unix_tests { fn t() { std::fs::create_dir("u").ok(); } }
fn tail() {}
"##;
    let (code, declared) = product(fixture);
    for kept in [
        "fn before",
        "fn after",
        "DirBuilder::new(",
        "fn last",
        "kept: u8",
        "fn call",
        "fn tail",
    ] {
        assert!(code.contains(kept), "{kept} was cut: {code}");
    }
    for cut in [
        "braces_in_text",
        "nested",
        "inner",
        "helper",
        "hand",
        "unix_tests",
        "create_dir(",
    ] {
        assert!(!code.contains(cut), "{cut} was kept: {code}");
    }
    assert_eq!(code.matches("create_dir_all(").count(), 2, "{code}");
    assert_eq!(declared, ["declared"]);
    assert_eq!(code.lines().count(), fixture.lines().count(), "lines kept");
}

#[test]
fn test_files_are_known_by_their_declarations() {
    let tree = product_tree();
    for test_file in [
        "src/profile_home/tests.rs",
        "src/profile_home/tests/scan.rs",
        "src/session/store/fork/tests.rs",
        "src/mcp/tests.rs",
        "src/test_net.rs",
    ] {
        assert!(!tree.contains_key(test_file), "{test_file} read as product");
    }
    for product_file in [
        "src/profile_home.rs",
        "src/session/store/fork.rs",
        "src/lib.rs",
    ] {
        assert!(tree.contains_key(product_file), "{product_file} not read");
    }
}

#[test]
fn no_product_code_makes_a_directory_but_on_the_lines_allowed() {
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut offenders = Vec::new();
    for (file, code) in product_tree() {
        for (number, line) in code.lines().enumerate() {
            if !BANNED.iter().any(|banned| line.contains(banned)) {
                continue;
            }
            let key = (file.clone(), line.trim().to_string());
            if ALLOWED.iter().any(|(f, l)| *f == key.0 && *l == key.1) {
                *seen.entry(key).or_default() += 1;
            } else {
                offenders.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
    // And every line allowed is there, once: an allowance a change made stale
    // would let the next one through unread.
    for (file, line) in ALLOWED {
        assert_eq!(
            seen.get(&(file.to_string(), line.to_string())),
            Some(&1),
            "{file}: {line}"
        );
    }
}

#[test]
fn every_profile_writer_makes_its_directories_through_the_helper() {
    let tree = product_tree();
    let code = |file: &str| tree.get(file).unwrap_or_else(|| panic!("{file}")).clone();
    for (file, through, count) in [
        (
            "src/provider/profile.rs",
            "crate::profile_home::ensure(dir, dir)?",
            5,
        ),
        (
            "src/mcp/config.rs",
            "crate::profile_home::ensure(dir, dir)",
            1,
        ),
        (
            "src/session/prompt_history.rs",
            "crate::profile_home::ensure(&self.dir, &self.dir)?",
            1,
        ),
        (
            "src/skills/command.rs",
            "super::install::ensure_managed(managed, &dir)",
            1,
        ),
        (
            "src/skills/install.rs",
            "crate::profile_home::ensure_content(profile_of(managed)?, dir)",
            1,
        ),
        (
            "src/skills/install.rs",
            "ensure_managed(managed, managed)?",
            2,
        ),
        // The lock directory is private state; the transaction and every
        // directory the copy makes are new.
        (
            "src/skills/install.rs",
            "crate::profile_home::ensure(parent, &dir)?",
            1,
        ),
        ("src/skills/install.rs", "crate::profile_home::make_new(", 2),
        (
            "src/session/store.rs",
            "crate::profile_home::ensure(&store.profile_dir, &store.profile_dir)",
            1,
        ),
        (
            "src/session/store.rs",
            "crate::profile_home::problem(path)",
            1,
        ),
    ] {
        assert_eq!(
            code(file).matches(through).count(),
            count,
            "{file}: {through}"
        );
    }
}
