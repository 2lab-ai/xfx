//! The code fence language registry (`code_highlight_languages.zig`): which
//! labels name which tokenizer profile, and the few source shapes confident
//! enough to infer a profile from an unlabeled block.

use std::collections::HashSet;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordCase {
    Sensitive,
    AsciiInsensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockComment {
    pub start: &'static str,
    pub end: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    None,
    TypescriptAssertion,
    Json,
    ShellShebang,
    PythonHeader,
    SqlSelect,
    DockerfileFrom,
    GoPackage,
    RustFunction,
    DiffPatch,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Profile {
    pub label: &'static str,
    pub aliases: &'static [&'static str],
    pub line_comments: &'static [&'static str],
    pub block_comment: Option<BlockComment>,
    pub quotes: &'static [u8],
    /// Characters coloured as operator runs.
    pub operators: &'static [u8],
    /// `$name`-style variables take the variable colour.
    pub dollar_vars: bool,
    /// A dash at a word boundary opens a flag token (`-n`, `--json`) in the
    /// number colour.
    pub dash_flags: bool,
    /// The first word of each command (after start, a pipe or logical
    /// operator, `;`, `&`, `$(`, or a control keyword) is coloured as a
    /// command. Mirrors how the bash grammar scopes command words vs
    /// arguments.
    pub command_words: bool,
    /// When false, bare number arguments stay plain and only file
    /// descriptors glued to a redirect take the number colour.
    pub bare_numbers: bool,
    /// Diff patches paint line-wise (+/-, hunks, file headers) instead of
    /// running the tokenizer.
    pub diff_lines: bool,
    /// Packed words: per word, its [`packed_word_hash`] as a little-endian
    /// `u32`, its length as one byte, then its bytes.
    pub keywords: &'static [u8],
    pub literals: &'static [u8],
    pub keyword_case: KeywordCase,
    pub detection: Detection,
}

/// Upstream's field defaults (`code_highlight_languages.zig:28-55`).
const BASE: Profile = Profile {
    label: "",
    aliases: &[],
    line_comments: &[],
    block_comment: None,
    quotes: &[],
    operators: &[],
    dollar_vars: false,
    dash_flags: false,
    command_words: false,
    bare_numbers: true,
    diff_lines: false,
    keywords: &[],
    literals: &[],
    keyword_case: KeywordCase::Sensitive,
    detection: Detection::None,
};

const DOUBLE_QUOTE: &[u8] = b"\"";
const DOUBLE_SINGLE_QUOTES: &[u8] = b"\"'";
const SHELL_QUOTES: &[u8] = b"\"'`";

const C_BLOCK: Option<BlockComment> = Some(BlockComment {
    start: "/*",
    end: "*/",
});
const MARKUP_BLOCK: Option<BlockComment> = Some(BlockComment {
    start: "<!--",
    end: "-->",
});

/// A lookup filter only: callers compare exact bytes after a hash match.
pub const fn packed_word_hash(word: &[u8]) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    let mut i = 0;
    while i < word.len() {
        hash = (hash ^ word[i].to_ascii_lowercase() as u32).wrapping_mul(16_777_619);
        i += 1;
    }
    hash
}

const fn packed_words_len(words: &[&str]) -> usize {
    let mut len = 0;
    let mut i = 0;
    while i < words.len() {
        assert!(
            words[i].len() <= u8::MAX as usize,
            "syntax word is too long"
        );
        len += 4 + 1 + words[i].len();
        i += 1;
    }
    len
}

const fn pack_words<const N: usize>(words: &[&str]) -> [u8; N] {
    let mut result = [0u8; N];
    let mut offset = 0;
    let mut i = 0;
    while i < words.len() {
        let word = words[i].as_bytes();
        let hash = packed_word_hash(word).to_le_bytes();
        let mut h = 0;
        while h < 4 {
            result[offset + h] = hash[h];
            h += 1;
        }
        offset += 4;
        result[offset] = word.len() as u8;
        offset += 1;
        let mut b = 0;
        while b < word.len() {
            result[offset + b] = word[b];
            b += 1;
        }
        offset += word.len();
        i += 1;
    }
    assert!(offset == N, "packed word length mismatch");
    result
}

/// Packs a word list at compile time, as upstream's `packWords` does.
macro_rules! words {
    ($($word:literal),* $(,)?) => {{
        const WORDS: &[&str] = &[$($word),*];
        const PACKED: [u8; packed_words_len(WORDS)] = pack_words(WORDS);
        &PACKED
    }};
}

static PROFILES: &[Profile] = &[
    Profile {
        label: "zig",
        aliases: &["zig"],
        line_comments: &["//"],
        quotes: DOUBLE_QUOTE,
        keywords: words!(
            "const", "var", "fn", "pub", "return", "if", "else", "while", "for", "struct", "enum",
            "union", "try", "catch", "comptime", "defer", "errdefer", "async", "await", "anytype",
            "void"
        ),
        ..BASE
    },
    Profile {
        label: "ts",
        aliases: &["js", "jsx", "javascript", "ts", "tsx", "typescript"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: SHELL_QUOTES,
        keywords: words!(
            "const",
            "let",
            "var",
            "function",
            "class",
            "interface",
            "type",
            "export",
            "import",
            "from",
            "return",
            "if",
            "else",
            "for",
            "while",
            "async",
            "await",
            "new",
            "extends",
            "implements",
            "public",
            "private",
            "readonly"
        ),
        literals: words!("true", "false", "null", "undefined"),
        detection: Detection::TypescriptAssertion,
        ..BASE
    },
    Profile {
        label: "json",
        aliases: &["json"],
        quotes: DOUBLE_QUOTE,
        literals: words!("true", "false", "null"),
        detection: Detection::Json,
        ..BASE
    },
    Profile {
        label: "sh",
        aliases: &["sh", "bash", "zsh", "shell", "shellscript"],
        line_comments: &["#"],
        // Backticks are code, not strings, in shell.
        quotes: DOUBLE_SINGLE_QUOTES,
        operators: b"&|;<>*",
        dollar_vars: true,
        dash_flags: true,
        command_words: true,
        bare_numbers: false,
        detection: Detection::ShellShebang,
        ..BASE
    },
    Profile {
        label: "python",
        aliases: &["python", "py"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "def", "class", "return", "if", "elif", "else", "for", "while", "in", "import", "from",
            "as", "try", "except", "with", "lambda", "async", "await", "pass", "raise", "yield",
            "match", "case"
        ),
        literals: words!("True", "False", "None"),
        detection: Detection::PythonHeader,
        ..BASE
    },
    Profile {
        label: "yaml",
        aliases: &["yaml", "yml"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        literals: words!("true", "false", "null", "yes", "no", "on", "off"),
        ..BASE
    },
    Profile {
        label: "toml",
        aliases: &["toml"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        literals: words!("true", "false"),
        ..BASE
    },
    Profile {
        label: "sql",
        aliases: &["sql"],
        line_comments: &["--"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "select", "from", "where", "join", "left", "right", "inner", "outer", "on", "insert",
            "into", "values", "update", "set", "delete", "create", "alter", "drop", "table",
            "index", "group", "by", "order", "having", "limit", "as", "and", "or", "not",
            "distinct", "union"
        ),
        literals: words!("true", "false", "null"),
        keyword_case: KeywordCase::AsciiInsensitive,
        detection: Detection::SqlSelect,
        ..BASE
    },
    Profile {
        label: "dockerfile",
        aliases: &["dockerfile", "docker"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "from",
            "run",
            "cmd",
            "entrypoint",
            "copy",
            "add",
            "workdir",
            "env",
            "arg",
            "expose",
            "volume",
            "user",
            "label",
            "onbuild",
            "stopsignal",
            "healthcheck",
            "shell",
            "maintainer"
        ),
        keyword_case: KeywordCase::AsciiInsensitive,
        detection: Detection::DockerfileFrom,
        ..BASE
    },
    Profile {
        label: "rust",
        aliases: &["rust", "rs"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use", "mod", "crate",
            "return", "if", "else", "match", "for", "while", "loop", "async", "await", "move",
            "where", "self", "super"
        ),
        literals: words!("true", "false", "None", "Some"),
        detection: Detection::RustFunction,
        ..BASE
    },
    Profile {
        label: "go",
        aliases: &["go"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: b"\"`",
        keywords: words!(
            "package",
            "import",
            "func",
            "var",
            "const",
            "type",
            "struct",
            "interface",
            "return",
            "if",
            "else",
            "for",
            "range",
            "switch",
            "case",
            "go",
            "defer",
            "select",
            "chan",
            "map"
        ),
        literals: words!("true", "false", "nil"),
        detection: Detection::GoPackage,
        ..BASE
    },
    Profile {
        label: "c",
        aliases: &["c", "h", "m", "mm"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
            "else", "enum", "extern", "float", "for", "goto", "if", "int", "long", "return",
            "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
            "unsigned", "void", "volatile", "while"
        ),
        literals: words!("true", "false", "NULL"),
        ..BASE
    },
    Profile {
        label: "cpp",
        aliases: &["cpp", "c++", "cc", "cxx", "hpp"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "auto",
            "bool",
            "class",
            "const",
            "constexpr",
            "decltype",
            "delete",
            "enum",
            "explicit",
            "friend",
            "inline",
            "namespace",
            "new",
            "nullptr",
            "private",
            "protected",
            "public",
            "template",
            "this",
            "typename",
            "using",
            "virtual",
            "void"
        ),
        literals: words!("true", "false", "nullptr", "NULL"),
        ..BASE
    },
    Profile {
        label: "csharp",
        aliases: &["csharp", "cs"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "class",
            "namespace",
            "using",
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "void",
            "string",
            "int",
            "var",
            "new",
            "return",
            "if",
            "else",
            "for",
            "foreach",
            "while",
            "async",
            "await",
            "interface",
            "record",
            "get",
            "set"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "java",
        aliases: &["java"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "class",
            "interface",
            "package",
            "import",
            "public",
            "private",
            "protected",
            "static",
            "final",
            "void",
            "new",
            "return",
            "if",
            "else",
            "for",
            "while",
            "try",
            "catch",
            "throws",
            "extends",
            "implements",
            "record",
            "var"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "kotlin",
        aliases: &["kotlin", "kt", "kts"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "fun",
            "val",
            "var",
            "class",
            "object",
            "interface",
            "package",
            "import",
            "public",
            "private",
            "return",
            "if",
            "else",
            "when",
            "for",
            "while",
            "try",
            "catch",
            "data",
            "sealed",
            "suspend"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "php",
        aliases: &["php"],
        line_comments: &["//", "#"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "function",
            "class",
            "public",
            "private",
            "protected",
            "namespace",
            "use",
            "return",
            "if",
            "else",
            "foreach",
            "for",
            "while",
            "try",
            "catch",
            "new",
            "static",
            "const",
            "echo",
            "yield"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "ruby",
        aliases: &["ruby", "rb"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "def",
            "class",
            "module",
            "end",
            "return",
            "if",
            "elsif",
            "else",
            "unless",
            "case",
            "when",
            "do",
            "while",
            "for",
            "in",
            "begin",
            "rescue",
            "require",
            "attr_reader"
        ),
        literals: words!("true", "false", "nil"),
        ..BASE
    },
    Profile {
        label: "swift",
        aliases: &["swift"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "func",
            "let",
            "var",
            "class",
            "struct",
            "enum",
            "protocol",
            "extension",
            "import",
            "public",
            "private",
            "return",
            "if",
            "else",
            "guard",
            "for",
            "while",
            "switch",
            "case",
            "async",
            "await",
            "throws",
            "try"
        ),
        literals: words!("true", "false", "nil"),
        ..BASE
    },
    Profile {
        label: "powershell",
        aliases: &["powershell", "ps1", "pwsh", "ps"],
        line_comments: &["#"],
        block_comment: Some(BlockComment {
            start: "<#",
            end: "#>",
        }),
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "function", "param", "if", "else", "elseif", "foreach", "for", "while", "switch",
            "return", "throw", "try", "catch", "finally", "begin", "process", "end", "filter",
            "class", "enum"
        ),
        literals: words!("true", "false", "null"),
        keyword_case: KeywordCase::AsciiInsensitive,
        ..BASE
    },
    Profile {
        label: "lua",
        aliases: &["lua"],
        line_comments: &["--"],
        block_comment: Some(BlockComment {
            start: "--[[",
            end: "]]",
        }),
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto",
            "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until",
            "while"
        ),
        literals: words!("true", "false", "nil"),
        ..BASE
    },
    Profile {
        label: "html",
        aliases: &["html", "htm", "vue", "svelte"],
        block_comment: MARKUP_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "html", "head", "body", "main", "header", "footer", "section", "article", "div",
            "span", "a", "p", "script", "style", "link", "meta", "title", "button", "input",
            "form", "img", "ul", "li"
        ),
        ..BASE
    },
    Profile {
        label: "xml",
        aliases: &["xml"],
        block_comment: MARKUP_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!("xml", "version", "encoding", "DOCTYPE", "CDATA"),
        ..BASE
    },
    Profile {
        label: "css",
        aliases: &["css"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "color",
            "background",
            "display",
            "position",
            "margin",
            "padding",
            "border",
            "font",
            "width",
            "height",
            "flex",
            "grid",
            "align",
            "justify",
            "transition",
            "transform",
            "animation",
            "media"
        ),
        ..BASE
    },
    Profile {
        label: "hcl",
        aliases: &["hcl", "terraform", "tf"],
        line_comments: &["#", "//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "resource",
            "module",
            "variable",
            "output",
            "provider",
            "terraform",
            "locals",
            "data",
            "dynamic",
            "for_each",
            "count"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "make",
        aliases: &["make", "makefile", "mk"],
        line_comments: &["#"],
        dollar_vars: true,
        ..BASE
    },
    Profile {
        label: "ini",
        aliases: &["ini", "conf", "cfg", "editorconfig"],
        line_comments: &["#", ";"],
        ..BASE
    },
    Profile {
        label: "dotenv",
        aliases: &["dotenv", "env"],
        line_comments: &["#"],
        ..BASE
    },
    Profile {
        label: "graphql",
        aliases: &["graphql", "gql"],
        line_comments: &["#"],
        quotes: DOUBLE_QUOTE,
        keywords: words!(
            "query",
            "mutation",
            "subscription",
            "fragment",
            "on",
            "type",
            "input",
            "interface",
            "enum",
            "union",
            "scalar",
            "schema",
            "extend",
            "implements",
            "directive"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "dart",
        aliases: &["dart"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "const",
            "final",
            "var",
            "class",
            "extends",
            "with",
            "implements",
            "mixin",
            "enum",
            "if",
            "else",
            "for",
            "while",
            "return",
            "async",
            "await",
            "new",
            "static",
            "import",
            "export",
            "void"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "scala",
        aliases: &["scala", "sc"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_QUOTE,
        keywords: words!(
            "val", "var", "def", "class", "object", "trait", "extends", "with", "package",
            "import", "if", "else", "for", "while", "yield", "match", "case", "return", "new",
            "type", "given", "override"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "elixir",
        aliases: &["elixir", "ex", "exs"],
        line_comments: &["#"],
        quotes: DOUBLE_QUOTE,
        keywords: words!(
            "def",
            "defmodule",
            "defp",
            "defmacro",
            "defguard",
            "do",
            "end",
            "fn",
            "if",
            "else",
            "unless",
            "case",
            "cond",
            "when",
            "with",
            "for",
            "try",
            "rescue",
            "after",
            "alias",
            "import",
            "require",
            "use"
        ),
        literals: words!("true", "false", "nil"),
        ..BASE
    },
    Profile {
        label: "haskell",
        aliases: &["haskell", "hs"],
        line_comments: &["--"],
        block_comment: Some(BlockComment {
            start: "{-",
            end: "-}",
        }),
        quotes: DOUBLE_QUOTE,
        keywords: words!(
            "module", "where", "import", "data", "type", "newtype", "class", "instance",
            "deriving", "if", "then", "else", "case", "of", "do", "let", "in", "infix", "infixl",
            "infixr"
        ),
        literals: words!("True", "False"),
        ..BASE
    },
    Profile {
        label: "perl",
        aliases: &["perl", "pl", "pm"],
        line_comments: &["#"],
        quotes: SHELL_QUOTES,
        dollar_vars: true,
        keywords: words!(
            "my", "our", "sub", "use", "package", "if", "else", "elsif", "unless", "while", "for",
            "foreach", "return", "local", "state", "say", "print", "die", "warn", "eval", "do",
            "require"
        ),
        literals: words!("undef"),
        ..BASE
    },
    Profile {
        label: "r",
        aliases: &["r"],
        line_comments: &["#"],
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "function", "if", "else", "for", "while", "repeat", "break", "next", "return", "in",
            "library", "require"
        ),
        literals: words!("TRUE", "FALSE", "NULL", "NA"),
        ..BASE
    },
    Profile {
        label: "groovy",
        aliases: &["groovy", "gradle"],
        line_comments: &["//"],
        block_comment: C_BLOCK,
        quotes: DOUBLE_SINGLE_QUOTES,
        keywords: words!(
            "def",
            "class",
            "interface",
            "enum",
            "if",
            "else",
            "for",
            "while",
            "return",
            "new",
            "try",
            "catch",
            "finally",
            "throw",
            "package",
            "import",
            "extends",
            "implements",
            "static",
            "final",
            "void"
        ),
        literals: words!("true", "false", "null"),
        ..BASE
    },
    Profile {
        label: "nginx",
        aliases: &["nginx"],
        line_comments: &["#"],
        keywords: words!(
            "server",
            "location",
            "listen",
            "root",
            "proxy_pass",
            "set",
            "return",
            "rewrite",
            "if",
            "error_page",
            "access_log",
            "include",
            "upstream",
            "worker_processes",
            "events",
            "http"
        ),
        ..BASE
    },
    Profile {
        // Inline code spans colour as strings; prose numbers stay plain.
        label: "markdown",
        aliases: &["md", "markdown", "mdx"],
        block_comment: MARKUP_BLOCK,
        quotes: b"`",
        bare_numbers: false,
        ..BASE
    },
    Profile {
        // Explicit opt-out of highlighting; kept byte-identical.
        label: "text",
        aliases: &["text", "txt", "plain", "plaintext"],
        bare_numbers: false,
        ..BASE
    },
    Profile {
        label: "diff",
        aliases: &["diff", "patch"],
        diff_lines: true,
        detection: Detection::DiffPatch,
        ..BASE
    },
];

/// The profile a fence label names, case-insensitively.
pub fn resolve(label: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|profile| {
        profile
            .aliases
            .iter()
            .any(|alias| label.eq_ignore_ascii_case(alias))
    })
}

/// The first profile whose detection matches `source`.
pub fn infer(source: &str) -> Option<&'static Profile> {
    PROFILES
        .iter()
        .find(|profile| matches_detection(profile.detection, source))
}

fn matches_detection(detection: Detection, source: &str) -> bool {
    match detection {
        Detection::None => false,
        Detection::TypescriptAssertion => matches_typescript_assertion(source),
        Detection::Json => is_valid_json(source),
        Detection::ShellShebang => matches_shell_shebang(source),
        Detection::PythonHeader => matches_python_header(source),
        Detection::SqlSelect => matches_sql_select(source),
        Detection::DockerfileFrom => starts_with_ignore_case(first_nonblank_line(source), "from "),
        Detection::GoPackage => {
            first_nonblank_line(source).starts_with("package ")
                && contains_line_start(source, "func ")
        }
        Detection::RustFunction => matches_rust_function(source),
        Detection::DiffPatch => matches_diff_patch(source),
    }
}

fn matches_diff_patch(source: &str) -> bool {
    let line = first_nonblank_line(source);
    if line.starts_with("diff --git ") || line.starts_with("@@ ") {
        return true;
    }
    line.starts_with("--- ") && source.contains("\n+++ ")
}

fn matches_typescript_assertion(source: &str) -> bool {
    const ASSERTION: &str = "} as ";
    let bytes = source.as_bytes();
    let mut start = 0;
    while let Some(offset) = source[start..].find(ASSERTION) {
        let type_start = start + offset + ASSERTION.len();
        if type_start < bytes.len() && bytes[type_start].is_ascii_uppercase() {
            return true;
        }
        start = type_start;
    }
    false
}

/// `std.json.parseFromSlice(std.json.Value, ...)` accepting an object or an
/// array. Like upstream's default parse options, a duplicate object key
/// makes the document invalid.
fn is_valid_json(source: &str) -> bool {
    let trimmed = source.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return false;
    }
    serde_json::from_str::<StrictJson>(trimmed).is_ok()
}

/// Any JSON value, rejecting duplicate object keys at every depth.
struct StrictJson;

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJson;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_unit<E: de::Error>(self) -> Result<StrictJson, E> {
        Ok(StrictJson)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<StrictJson, A::Error> {
        while seq.next_element::<StrictJson>()?.is_some() {}
        Ok(StrictJson)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StrictJson, A::Error> {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate field"));
            }
            map.next_value::<StrictJson>()?;
        }
        Ok(StrictJson)
    }
}

fn matches_shell_shebang(source: &str) -> bool {
    let line = first_nonblank_line(source);
    line.starts_with("#!")
        && (line.contains("bash") || line.contains("zsh") || line.contains("/sh"))
}

fn matches_python_header(source: &str) -> bool {
    let line = first_nonblank_line(source);
    (line.starts_with("def ") || line.starts_with("class ")) && line.ends_with(':')
}

fn matches_sql_select(source: &str) -> bool {
    let line = first_nonblank_line(source);
    starts_with_ignore_case(line, "select ") && contains_word_ignore_case(source, "from")
}

fn matches_rust_function(source: &str) -> bool {
    let line = first_nonblank_line(source);
    if !line.starts_with("fn ") && !line.starts_with("pub fn ") {
        return false;
    }
    source.contains("let ") || source.contains("println!") || line.contains("->")
}

fn first_nonblank_line(source: &str) -> &str {
    source
        .split('\n')
        .map(|line| line.trim_matches(|c| matches!(c, ' ' | '\t' | '\r')))
        .find(|trimmed| !trimmed.is_empty())
        .unwrap_or("")
}

fn contains_line_start(source: &str, prefix: &str) -> bool {
    source.starts_with(prefix)
        || source
            .split('\n')
            .skip(1)
            .any(|line| line.starts_with(prefix))
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn contains_word_ignore_case(source: &str, word: &str) -> bool {
    let source = source.as_bytes();
    let word = word.as_bytes();
    if word.len() > source.len() {
        return false;
    }
    (0..=source.len() - word.len()).any(|index| {
        let end = index + word.len();
        source[index..end].eq_ignore_ascii_case(word)
            && (index == 0 || !is_word_byte(source[index - 1]))
            && (end == source.len() || !is_word_byte(source[end]))
    })
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_syntax_words_retain_exact_hashes_and_boundaries() {
        for profile in PROFILES {
            for words in [profile.keywords, profile.literals] {
                let mut offset = 0;
                while offset < words.len() {
                    let hash = u32::from_le_bytes([
                        words[offset],
                        words[offset + 1],
                        words[offset + 2],
                        words[offset + 3],
                    ]);
                    offset += 4;
                    let len = usize::from(words[offset]);
                    offset += 1;
                    let end = offset + len;
                    assert!(end <= words.len());
                    assert!(len > 0);
                    assert_eq!(hash, packed_word_hash(&words[offset..end]));
                    offset = end;
                }
                assert_eq!(words.len(), offset);
            }
        }
    }

    #[test]
    fn typescript_assertions_infer_the_canonical_typescript_label() {
        let source = "const hook = await resumeHook(token, { cleanup: true } as CleanupSignal);";

        assert_eq!(infer(source).map(|profile| profile.label), Some("ts"));
        assert!(infer("const value = 1;").is_none());
        assert!(infer("const value = {} as cleanupSignal;").is_none());
    }

    #[test]
    fn supported_code_fence_labels_resolve_case_insensitively() {
        let cases = [
            ("Zig", "zig"),
            ("js", "ts"),
            ("JSX", "ts"),
            ("javascript", "ts"),
            ("TS", "ts"),
            ("tsx", "ts"),
            ("TypeScript", "ts"),
            ("JSON", "json"),
            ("sh", "sh"),
            ("BASH", "sh"),
            ("zsh", "sh"),
            ("Shell", "sh"),
        ];
        for (label, expected) in cases {
            assert_eq!(
                resolve(label).map(|profile| profile.label),
                Some(expected),
                "{label}"
            );
        }
        assert!(resolve("").is_none());
        // text resolves to a deliberate plain profile; rendering stays
        // byte-identical.
        assert_eq!(resolve("text").map(|profile| profile.label), Some("text"));
    }

    #[test]
    fn expanded_code_fence_labels_resolve_through_the_language_registry() {
        let cases = [
            ("python", "python"),
            ("py", "python"),
            ("yaml", "yaml"),
            ("yml", "yaml"),
            ("toml", "toml"),
            ("sql", "sql"),
            ("dockerfile", "dockerfile"),
            ("rust", "rust"),
            ("rs", "rust"),
            ("go", "go"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("c++", "cpp"),
            ("csharp", "csharp"),
            ("cs", "csharp"),
            ("java", "java"),
            ("kotlin", "kotlin"),
            ("php", "php"),
            ("ruby", "ruby"),
            ("swift", "swift"),
            ("powershell", "powershell"),
            ("ps1", "powershell"),
            ("lua", "lua"),
            ("html", "html"),
            ("xml", "xml"),
            ("css", "css"),
            ("hcl", "hcl"),
            ("terraform", "hcl"),
            ("tf", "hcl"),
        ];
        for (label, expected) in cases {
            assert_eq!(
                resolve(label).map(|profile| profile.label),
                Some(expected),
                "{label}"
            );
        }
    }

    #[test]
    fn high_confidence_source_shapes_infer_registered_profiles() {
        let cases = [
            ("{\"ready\": true}", "json"),
            ("#!/usr/bin/env bash\necho ready", "sh"),
            ("def render(value):\n    return value", "python"),
            ("SELECT id FROM users", "sql"),
            ("FROM alpine:3.20\nRUN echo ready", "dockerfile"),
            ("package main\nfunc main() {}", "go"),
            ("fn main() { println!(\"ready\"); }", "rust"),
        ];

        for (source, expected) in cases {
            assert_eq!(
                infer(source).map(|profile| profile.label),
                Some(expected),
                "{source}"
            );
        }
        assert!(infer("const value = 1;").is_none());
        assert!(infer("title: ready").is_none());
    }

    #[test]
    fn aliases_do_not_collide_across_profiles() {
        for (profile_index, profile) in PROFILES.iter().enumerate() {
            for alias in profile.aliases {
                for other in &PROFILES[profile_index + 1..] {
                    for other_alias in other.aliases {
                        assert!(
                            !alias.eq_ignore_ascii_case(other_alias),
                            "{alias} / {other_alias}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn resolve_covers_the_added_languages_and_aliases() {
        let cases = [
            ("makefile", "make"),
            ("conf", "ini"),
            ("env", "dotenv"),
            ("gql", "graphql"),
            ("dart", "dart"),
            ("sc", "scala"),
            ("exs", "elixir"),
            ("hs", "haskell"),
            ("pl", "perl"),
            ("r", "r"),
            ("gradle", "groovy"),
            ("nginx", "nginx"),
            ("md", "markdown"),
            ("txt", "text"),
            ("patch", "diff"),
            ("shellscript", "sh"),
            ("mm", "c"),
            ("vue", "html"),
        ];
        for (alias, label) in cases {
            assert_eq!(
                resolve(alias).map(|profile| profile.label),
                Some(label),
                "{alias}"
            );
        }
    }

    #[test]
    fn infer_detects_diff_patches_without_a_fence_label() {
        let profile = infer("--- a/main.zig\n+++ b/main.zig\n@@ -1 +1 @@\n-old\n+new");
        assert_eq!(profile.map(|profile| profile.label), Some("diff"));
        assert!(infer("plain prose about --- things").is_none());
    }
}
