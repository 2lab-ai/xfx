"""TC-NOTATION's mutants, applied to a throwaway clone of the head.

usage: python3 -I mutate.py <clone root> <mutant name>
Each mutant is one or more exact replacements (each must match once).
"""
import sys, pathlib
R = pathlib.Path(sys.argv[1])
name = sys.argv[2]
def rep(path, old, new):
    p = R / path
    s = p.read_text()
    assert s.count(old) == 1, (name, path, s.count(old))
    p.write_text(s.replace(old, new))
M = {
 # A: one palette grey (the divider's dark 240) spelt direct again
 "A-divider-direct": [("src/tui/theme.rs",
   '        240 => "\\u{1b}[38;5;240m",',
   '        240 => "\\u{1b}[38;2;88;88;88m",')],
 # B: the statusline/dim grey 245 spelt direct again (summary, hint row, tool rows)
 "B-245-direct": [("src/tui/theme.rs",
   '        245 => "\\u{1b}[38;5;245m",',
   '        245 => "\\u{1b}[38;2;138;138;138m",')],
 # C: the bold notice heading spelt direct on a dark screen
 "C-heading-direct": [("src/tui/theme.rs",
   '            Mode::Dark => "\\u{1b}[1;38;5;252m",',
   '            Mode::Dark => "\\u{1b}[1;38;2;208;208;208m",')],
 # D: the welcome brand spelt direct on a light screen
 "D-subtitle-direct": [("src/tui/theme.rs",
   '            Mode::Light => "\\u{1b}[1;38;5;235m",',
   '            Mode::Light => "\\u{1b}[1;38;2;38;38;38m",')],
 # E: the file picker's bar spelt direct on a dark screen
 "E-background-direct": [("src/tui/theme.rs",
   '            Mode::Dark => "\\u{1b}[48;5;239m",',
   '            Mode::Dark => "\\u{1b}[48;2;78;78;78m",')],
 # F: a direct ramp grey on the screen read as the index it spells (the base's TC reading)
 "F-retint-takes-direct": [("src/tui/theme.rs",
   "    ) -> Option<&'static str> {\n        let roles: &[usize] = match row {",
   "    ) -> Option<&'static str> {\n        let colour = match colour {\n            Color::Rgb(l, g, b) if l == g && g == b && l >= 8 && l <= 238 && (l - 8) % 10 == 0 => Color::Indexed(232 + (l - 8) / 10),\n            other => other,\n        };\n        let roles: &[usize] = match row {"),
  ("src/tui/theme.rs",
   "    pub(crate) fn document_retint(&self, colour: Color) -> Option<&'static str> {\n",
   "    pub(crate) fn document_retint(&self, colour: Color) -> Option<&'static str> {\n        let colour = match colour {\n            Color::Rgb(l, g, b) if l == g && g == b && l >= 8 && l <= 238 && (l - 8) % 10 == 0 => Color::Indexed(232 + (l - 8) / 10),\n            other => other,\n        };\n")],
 # G: a row written recoloured reads a direct ramp grey as its index again
 "G-recolour-takes-direct": [("src/tui/transcript.rs",
   '    let ["38", "5", index] = params.as_slice() else {\n        return None;\n    };\n    let colour = Color::Indexed(number(index)?);',
   '    let colour = match params.as_slice() {\n        ["38", "5", index] => Color::Indexed(number(index)?),\n        ["38", "2", l, _, _] => Color::Indexed(232 + (number(l)? - 8) / 10),\n        _ => return None,\n    };')],
 # H: the render allowlist admits a direct background again
 "H-allowlist-direct-background": [("src/tui/pacer.rs",
   '                    ("38", Some("2")) => 3,',
   '                    (_, Some("2")) => 3,')],
 # I: the wire parser admits a direct background again
 "I-wire-direct-background": [("src/tui/check.rs",
   '            "48" => match parts.next() {\n                Some("5") => {\n                    number(parts.next().ok_or_else(malformed)?).ok_or_else(malformed)?;\n                }',
   '            "48" => match parts.next() {\n                Some("5") => {\n                    number(parts.next().ok_or_else(malformed)?).ok_or_else(malformed)?;\n                }\n                Some("2") => {\n                    for _ in 0..3 { number(parts.next().ok_or_else(malformed)?).ok_or_else(malformed)?; }\n                }')],
}
for path, old, new in M[name]:
    rep(path, old, new)
print("applied", name)
