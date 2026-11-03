//! The `include.path` and `includeIf.<condition>.path` values of one git
//! configuration file, as git's own parser reads them.
//!
//! Used to make what the user's global git configuration includes part of the
//! protected set ([`super::links`]): an included file *is* that configuration.
//! Every `includeIf` counts, whatever its condition -- a condition that does
//! not hold today may hold in the next repository.
//!
//! The grammar is git's (`git-config(1)`, "Syntax"): `[section]`,
//! `[section "subsection"]` and the old `[section.subsection]`; names
//! case-insensitive; `name = value` or a bare `name`; `#` and `;` comments
//! outside quotes; double quotes; the escapes `\\`, `\"`, `\n`, `\t`, `\b`;
//! a backslash at the end of a line continuing the value. Anything git would
//! reject is an error here too, so the caller asks rather than guesses.

/// Every include path in `text`, in order, as written (not yet expanded).
pub(crate) fn include_paths(text: &str) -> Result<Vec<String>, String> {
    let text = text.replace("\r\n", "\n");
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
    };
    // The section's lower-case name, and whether it has a subsection.
    let mut section: Option<(String, bool)> = None;
    let mut paths = Vec::new();
    loop {
        parser.skip_blank();
        let Some(next) = parser.peek() else {
            break;
        };
        match next {
            '#' | ';' => parser.skip_line(),
            '[' => section = Some(parser.header()?),
            c if c.is_ascii_alphabetic() => {
                let key = parser.key();
                let value = parser.value()?;
                // `include.path`, and `includeIf.<condition>.path`: git's two
                // include keys, and no other spelling of them.
                let including = matches!(
                    section.as_ref().map(|(name, sub)| (name.as_str(), *sub)),
                    Some(("include", false) | ("includeif", true))
                );
                if including && key == "path" {
                    match value {
                        Some(value) => paths.push(value),
                        None => return Err(parser.error("an include `path` with no value")),
                    }
                }
            }
            other => return Err(parser.error(&format!("unexpected `{other}`"))),
        }
    }
    Ok(paths)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    line: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let next = self.peek()?;
        self.at += 1;
        if next == '\n' {
            self.line += 1;
        }
        Some(next)
    }

    fn error(&self, what: &str) -> String {
        format!("line {}: {what}", self.line)
    }

    fn skip_blank(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn skip_line(&mut self) {
        while let Some(next) = self.bump() {
            if next == '\n' {
                break;
            }
        }
    }

    /// `[name]`, `[name "sub"]` or `[name.sub]`: the section's name, lower
    /// case, and whether a subsection follows it.
    fn header(&mut self) -> Result<(String, bool), String> {
        self.bump();
        let mut name = String::new();
        while let Some(next) = self.peek() {
            if next.is_ascii_alphanumeric() || next == '-' || next == '.' {
                name.push(next.to_ascii_lowercase());
                self.bump();
            } else {
                break;
            }
        }
        let mut sub = false;
        if let Some((section, _)) = name.split_once('.') {
            name = section.to_string();
            sub = true;
        }
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.bump();
        }
        if self.peek() == Some('"') {
            sub = true;
            self.bump();
            loop {
                match self.bump() {
                    Some('"') => break,
                    Some('\\') => {
                        if self.bump().is_none_or(|c| c == '\n') {
                            return Err(self.error("an unterminated subsection"));
                        }
                    }
                    Some('\n') | None => return Err(self.error("an unterminated subsection")),
                    Some(_) => {}
                }
            }
        }
        if name.is_empty() || self.bump() != Some(']') {
            return Err(self.error("a malformed section header"));
        }
        Ok((name, sub))
    }

    fn key(&mut self) -> String {
        let mut key = String::new();
        while let Some(next) = self.peek() {
            if next.is_ascii_alphanumeric() || next == '-' {
                key.push(next.to_ascii_lowercase());
                self.bump();
            } else {
                break;
            }
        }
        key
    }

    /// What follows a key: `= value`, or nothing (a boolean). Git's
    /// `parse_value`: leading blanks skipped, blanks between words kept as
    /// spaces, trailing ones dropped, quotes and escapes resolved.
    fn value(&mut self) -> Result<Option<String>, String> {
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.bump();
        }
        match self.peek() {
            None | Some('\n') => return Ok(None),
            Some('#' | ';') => {
                self.skip_line();
                return Ok(None);
            }
            Some('=') => {
                self.bump();
            }
            Some(other) => return Err(self.error(&format!("unexpected `{other}` after a key"))),
        }
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.bump();
        }
        let mut value = String::new();
        let mut quoted = false;
        let mut spaces = 0;
        while let Some(next) = self.bump() {
            match next {
                '\n' if quoted => return Err(self.error("an unterminated quote")),
                '\n' => break,
                '#' | ';' if !quoted => {
                    self.skip_line();
                    break;
                }
                c if !quoted && c.is_whitespace() => spaces += 1,
                c => {
                    if spaces > 0 && !value.is_empty() {
                        value.extend(std::iter::repeat_n(' ', spaces));
                    }
                    spaces = 0;
                    match c {
                        '"' => quoted = !quoted,
                        '\\' => match self.bump() {
                            Some('\n') => {}
                            Some('\\') => value.push('\\'),
                            Some('"') => value.push('"'),
                            Some('n') => value.push('\n'),
                            Some('t') => value.push('\t'),
                            Some('b') => value.push('\u{8}'),
                            _ => return Err(self.error("an unknown escape")),
                        },
                        c => value.push(c),
                    }
                }
            }
        }
        if quoted {
            return Err(self.error("an unterminated quote"));
        }
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_include_and_include_if_path_is_read_as_git_reads_it() {
        // `include.sub.path` and a bare `includeIf.path` are not include keys.
        let text = "\
# a comment
[user]
\tname = Someone ; trailing
\tpath = not/an/include
[include]
\tpath = ~/.gitconfig.local
[includeIf \"gitdir:~/work/\"]
\tPATH = \"work dir/inc\" # quoted, with a space
[Include] path = same-line
[include.sub]
\tpath = not-an-include-key
[includeIf]
\tpath = nor-this
[include]
\tpath = con\\
tinued
[core]
\teditor = \"vim\" \\
\t  -f
[include]
\tpath = a  b\t
";
        assert_eq!(
            include_paths(text).expect("parses"),
            [
                "~/.gitconfig.local",
                "work dir/inc",
                "same-line",
                "continued",
                "a  b"
            ]
        );
    }

    #[test]
    fn what_git_would_reject_is_not_guessed_at() {
        for text in [
            "[include]\n\tpath = \"open\n",
            "[include\n",
            "[include]\n\tpath = a\\q\n",
            "[include]\n\t= value\n",
            "[include]\n\tpath\n",
        ] {
            assert!(include_paths(text).is_err(), "{text:?}");
        }
    }
}
