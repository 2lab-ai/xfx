"""The CALLROW-SAFE mutants, each applied alone to a throwaway clone.

usage: python3 -I mutants.py <clone> <name>    applies one; `restore` undoes all
"""
import subprocess
import sys

clone, name = sys.argv[1:3]
TOOLGROUP = "src/tui/toolgroup.rs"
PUSH = """    fn push(&mut self, character: char) -> bool {
        if character == '\\u{061c}' {
            return self.token("\\\\u{061c}");
        }
        let mut bytes = [0u8; 4];
        let token = crate::tools::question::terminal_safe(character.encode_utf8(&mut bytes));
        self.token(&token)
    }"""
MUTANTS = {
    # The marks are written as themselves: C0, DEL and C1 still named.
    "no-mark-naming": (TOOLGROUP, PUSH, """    fn push(&mut self, character: char) -> bool {
        // MUTANT no-mark-naming: C0, DEL and C1 named, no mark.
        let code = u32::from(character);
        if code < 0x20 || code == 0x7f {
            return self.token(&format!("\\\\x{code:02x}"));
        }
        if (0x80..=0x9f).contains(&code) {
            return self.token(&format!("\\\\u{{{code:04x}}}"));
        }
        let mut bytes = [0u8; 4];
        self.token(character.encode_utf8(&mut bytes))
    }"""),
    # Every control a space, as before: the marks still named.
    "no-control-spelling": (TOOLGROUP, PUSH, """    fn push(&mut self, character: char) -> bool {
        // MUTANT no-control-spelling: a control is a space, the marks named.
        if character.is_control() {
            return self.token(" ");
        }
        if character == '\\u{061c}' {
            return self.token("\\\\u{061c}");
        }
        let mut bytes = [0u8; 4];
        let token = crate::tools::question::terminal_safe(character.encode_utf8(&mut bytes));
        self.token(&token)
    }"""),
    # The channel blanks the argument again.
    "bridge-blanks": ("src/tui/bridge.rs", """        target: shown
            .target
            .map(|target| super::toolgroup::spelled(&target, activity)),""", """        // MUTANT bridge-blanks: the argument crosses blanked.
        target: shown.target.map(|target| {
            let _ = activity;
            inert_owned(target)
        }),"""),
    # A spelled value counts as shown by the spelled heading.
    "no-text-flag": ("src/tui/full_transcript.rs", """                Value::String(text) => (
                    text.clone(),
                    crate::tools::question::terminal_safe(text).len() == text.len()
                        && !text.contains('\\u{061c}')
                        && status
                            .as_deref()
                            .is_none_or(|status| status.contains(text.as_str())),
                ),""", """                // MUTANT no-text-flag: every string value may be shown already.
                Value::String(text) => {
                    let _ = &status;
                    (text.clone(), true)
                }"""),
    # Ctrl-O looks for the value in the whole heading, not fx's 120-byte status.
    "no-status-bound": ("src/tui/full_transcript.rs", """                        && !text.contains('\\u{061c}')
                        && status
                            .as_deref()
                            .is_none_or(|status| status.contains(text.as_str())),""", """                        // MUTANT no-status-bound: no status phrase looked in.
                        && !text.contains('\\u{061c}')
                        && status.as_deref().map_or(true, |_| true),"""),
    # The status phrase made from the channel's spelling, as round 1 made it.
    "status-from-spelled": ("src/tui/full_transcript.rs", """        .and_then(|spec| super::toolgroup::status(&spec.shown(input)));""", """        // MUTANT status-from-spelled: the bound applied to the spelled call.
        .and_then(|spec| {
            let mut shown = spec.shown(input);
            let activity = shown.presentation.activity;
            shown.target = shown
                .target
                .map(|target| super::toolgroup::spelled(&target, activity));
            super::toolgroup::status(&shown)
        });"""),
    # The source bound stops with no marker again, as fx's does.
    "no-source-cut-marker": ("src/tui/toolgroup.rs", """            {
                spelling.cut = true;
            }
            break;""", """            {
                // MUTANT no-source-cut-marker: nothing said of the tail.
            }
            break;"""),
    # The channel blanks the reason again.
    "reason-blanked": ("src/tui/bridge.rs", """                detail: super::toolgroup::flattened(&detail),""", """                // MUTANT reason-blanked: the reason crosses blanked.
                detail: inert_owned(detail),"""),
}
if name == "restore":
    subprocess.run(["git", "-C", clone, "checkout", "--", "src/"], check=True)
    sys.exit(0)
path, old, new = MUTANTS[name]
full = "%s/%s" % (clone, path)
text = open(full, encoding="utf-8").read()
assert text.count(old) == 1, (name, text.count(old))
open(full, "w", encoding="utf-8").write(text.replace(old, new))
print("applied", name, "to", path)
