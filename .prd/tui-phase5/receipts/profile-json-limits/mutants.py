"""Mutants of this unit's rules, each on a throwaway clone (`git clone
--no-local`), never the worktree: one edit at a time, the tests that should
see it run, the edit undone. A mutant is killed when a test fails, and
survives when every one passes; one that does not build is UNBUILT, no kill.

usage: python3 -I mutants.py <head repo> <clone dir>
  Each test run bounded at 900 s. Prints no path: the clone is <clone>.
"""

import os
import subprocess
import sys

sys.dont_write_bytecode = True

J = "src/config/layer_json.rs"
C = "src/config.rs"

LIB = ["--lib", "config::"]
CLI = ["--test", "cli", "ask_"]
M = "src/mcp/config.rs"
RT = "src/mcp/runtime.rs"
CHOICES_RT = ["--lib", "mcp::command::tests::saved_choices_are_read_as_fxs_parse_reads_them"]

# (name, file, old, new, test runs)
MUTANTS = [
    ("serde_json's parse back (the base)", C,
     "    match layer_json::object(&text) {\n        Some(object) => LayerRead::loaded(object),",
     "    match serde_json::from_str::<Value>(&text).ok().and_then(|v| match v { Value::Object(o) => Some(o), _ => None }) {\n        Some(object) => LayerRead::loaded(object),",
     [LIB, CLI]),
    ("keep one level less", J, "const KEPT_DEPTH: usize = 127;", "const KEPT_DEPTH: usize = 126;", [LIB]),
    ("keep one level more", J, "const KEPT_DEPTH: usize = 127;", "const KEPT_DEPTH: usize = 128;", [LIB]),
    ("keep every level", J, "const KEPT_DEPTH: usize = 127;", "const KEPT_DEPTH: usize = usize::MAX;", [LIB]),
    ("kept off by one", J, "let kept = open.len() <= KEPT_DEPTH;", "let kept = open.len() < KEPT_DEPTH;", [LIB]),
    ("past f64 is 0", J,
     "Value::from(if token.starts_with('-') {\n                -f64::MAX\n            } else {\n                f64::MAX\n            })",
     "Value::from(0)", [LIB, CLI]),
    ("past f64 is null", J,
     "Value::from(if token.starts_with('-') {\n                -f64::MAX\n            } else {\n                f64::MAX\n            })",
     "Value::Null", [LIB, CLI]),
    ("past f64 loses its sign", J, "                -f64::MAX\n", "                f64::MAX\n", [LIB]),
    ("past f64 refused", J,
     "Some(serde_json::from_str(token).unwrap_or_else(|_| {",
     "Some(serde_json::from_str::<Value>(token).ok()?).or_else(|| Some({", [LIB, CLI]),
    ("no number grammar", J, "        if !is_number(token.as_bytes()) {\n            return None;\n        }\n", "", [LIB]),
    ("a fraction needs no digit", J,
     "        at += 1;\n        if !digit(at) {\n            return false;\n        }\n        while digit(at) {\n            at += 1;\n        }\n    }\n    if matches!",
     "        at += 1;\n        while digit(at) {\n            at += 1;\n        }\n    }\n    if matches!", [LIB]),
    ("a leading zero is a digit", J, "        Some(b'0') => at += 1,\n", "", [LIB]),
    ("no `+` in a number", J, "Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')", "Some(b'0'..=b'9' | b'-' | b'.' | b'e' | b'E')", [LIB]),
    ("a key twice taken", J, "                        if !keys.insert(key.clone()) {", "                        if keys.insert(key.clone()) && false {", [LIB, CLI]),
    ("a key twice taken past the depth kept", J,
     "                        if !keys.insert(key.clone()) {",
     "                        if !keys.insert(key.clone()) && kept {", [LIB]),
    ("an escape is one byte", J, "                b'\\\\' => at += 2,", "                b'\\\\' => at += 1,", [LIB]),
    ("a string undecoded", J,
     "        serde_json::from_str(&self.text[start..self.at]).ok()",
     "        Some(self.text[start + 1..self.at - 1].to_string())", [LIB]),
    ("anything after the value", J,
     "                    return (reader.at == text.len()).then_some(done);",
     "                    return Some(done);", [LIB]),
    ("no carriage return in space", J, "Some(b' ' | b'\\t' | b'\\n' | b'\\r')", "Some(b' ' | b'\\t' | b'\\n')", [LIB]),
    ("a trailing comma in an array", J,
     "                    if reader.eat(b',') {\n                        break;\n                    }\n                    if !reader.eat(b']') {",
     "                    if reader.eat(b',') {\n                        reader.skip_space();\n                        if reader.eat(b']') { open.pop(); done = Value::Array(Vec::new()); continue; }\n                        break;\n                    }\n                    if !reader.eat(b']') {",
     [LIB]),
    ("no colon needed", J, "        self.eat(b':').then_some(key)", "        self.eat(b':');\n        Some(key)", [LIB]),
    # Round 2: the saved MCP choices (S1) and the label of bytes that are not
    # UTF-8 (N2).
    ("choices read by serde_json again", M,
     "    let document = match crate::config::read_profile_object(path) {",
     "    let document = match crate::provider::profile::read_existing(path).map(Some).map_err(|_| ()) {",
     [LIB, CHOICES_RT]),
    ("a failed load is no choice (the base's swallow)", M,
     "        Err(_) => return None,\n    };\n    fx_takes_choices",
     "        Err(_) => return Some(Choices::default()),\n    };\n    fx_takes_choices",
     [LIB, CHOICES_RT]),
    ("fx's checks skipped", M,
     "    fx_takes_choices(&document, root).then(|| choices_in(&document, root))",
     "    Some(choices_in(&document, root))",
     [LIB, CHOICES_RT]),
    ("workspaces need not be an object", M,
     "        Some(Value::Object(workspaces)) => root.to_str().and_then(|key| workspaces.get(key)),\n        Some(_) => return false,",
     "        Some(Value::Object(workspaces)) => root.to_str().and_then(|key| workspaces.get(key)),\n        Some(_) => return true,",
     [LIB]),
    ("the entry need not be an object", M,
     "        Some(Value::Object(entry)) => entry,\n        Some(_) => return false,",
     "        Some(Value::Object(entry)) => entry,\n        Some(_) => return true,",
     [LIB]),
    ("a list need not be an array", M,
     "            list.as_array().is_some_and(|names| {",
     "            list.as_array().is_none_or(|names| {",
     [LIB, CHOICES_RT]),
    ("a name need not be a string", M,
     ".all(|name| name.as_str().is_some_and(|name| !name.is_empty()))",
     ".all(|name| name.as_str().is_none_or(|name| !name.is_empty()))",
     [LIB]),
    ("a name may be empty", M,
     ".all(|name| name.as_str().is_some_and(|name| !name.is_empty()))",
     ".all(|name| name.as_str().is_some())",
     [LIB]),
    ("the rejections unchecked", M,
     "    names(ENABLED_KEY)\n        && names(DISABLED_KEY)\n",
     "    names(ENABLED_KEY)\n",
     [LIB]),
    ("the switch unchecked", M,
     "        && entry.get(ENABLE_ALL_KEY).is_none_or(Value::is_boolean)\n",
     "\n",
     [LIB, CHOICES_RT]),
    ("a profile that does not read is none", "src/config.rs",
     "        (None, Some(diagnostic)) => Err(diagnostic.cause),",
     "        (None, Some(_)) => Ok(None),",
     [LIB, CHOICES_RT]),
    ("the runtime loads the workspace anyway", RT,
     "        let choices = config::load_choices(self.paths.settings.as_deref(), &self.paths.workspace);\n",
     "        let choices = Some(config::load_choices(self.paths.settings.as_deref(), &self.paths.workspace).unwrap_or_default());\n",
     [CHOICES_RT]),
    ("not UTF-8 is unreadable again", "src/config.rs",
     "    let Ok(text) = String::from_utf8(bytes) else {\n        return reject(diagnostics, DiagnosticCause::MalformedSettings);",
     "    let Ok(text) = String::from_utf8(bytes) else {\n        return reject(diagnostics, DiagnosticCause::UnreadableSettings);",
     [LIB]),
    ("not UTF-8 decoded lossily", "src/config.rs",
     "    let Ok(text) = String::from_utf8(bytes) else {",
     "    let Ok(text) = Ok::<String, ()>(String::from_utf8_lossy(&bytes).into_owned()) else {",
     [LIB]),
    # Recursion where the reader keeps its own stack: the 100,000-deep read
    # runs the default thread's stack out (a crash is a kill).
    ("a recursive reader", J,
     "fn value(text: &str) -> Option<Value> {\n",
     "fn value(text: &str) -> Option<Value> {\n    fn rec(r: &mut Reader, d: usize) -> Option<Value> {\n        r.skip_space();\n        match r.peek()? {\n            b'[' => { r.at += 1; let mut v = Vec::new(); r.skip_space(); if r.eat(b']') { return Some(Value::Array(v)); } loop { let x = rec(r, d + 1)?; if d < KEPT_DEPTH { v.push(x); } r.skip_space(); if r.eat(b']') { return Some(Value::Array(v)); } if !r.eat(b',') { return None; } } }\n            b'{' => { r.at += 1; let mut m = Map::new(); let mut ks = HashSet::new(); r.skip_space(); if r.eat(b'}') { return Some(Value::Object(m)); } loop { r.skip_space(); let k = r.key()?; if !ks.insert(k.clone()) { return None; } let x = rec(r, d + 1)?; if d < KEPT_DEPTH { m.insert(k, x); } r.skip_space(); if r.eat(b'}') { return Some(Value::Object(m)); } if !r.eat(b',') { return None; } } }\n            _ => r.scalar(),\n        }\n    }\n    if !text.is_empty() {\n        let mut r = Reader { text, at: 0 };\n        let v = rec(&mut r, 1)?;\n        r.skip_space();\n        return (r.at == text.len()).then_some(v);\n    }\n",
     [LIB]),
]


def run(clone, args):
    command = ["perl", "-e", "alarm 900; exec @ARGV", "cargo", "test", "--locked", "-q", *args]
    done = subprocess.run(command, cwd=clone, capture_output=True, text=True)
    out = done.stdout + done.stderr
    # `-q` names a failed test only in the closing `failures:` list.
    failed, listing = [], False
    for line in out.splitlines():
        if line == "failures:":
            listing, failed = True, []
        elif listing and line.startswith("    ") and "::" in line or listing and line.startswith("    ") and line.strip().isidentifier():
            failed.append(line.strip())
        elif listing and line.strip() and not line.startswith("    "):
            listing = False
    crashed = "stack overflow" if "stack overflow" in out else ("a signal" if "signal:" in out else None)
    return done.returncode, failed, crashed, "error[" in out


def main():
    head, clone = sys.argv[1:3]
    if os.path.realpath(clone).startswith(os.path.realpath(head)):
        sys.exit("the clone must not be inside the worktree")
    subprocess.run(["rm", "-rf", clone], check=True)
    subprocess.run(["git", "clone", "-q", "--no-local", head, clone], check=True)
    rev = subprocess.run(["git", "-C", clone, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    print(f"clone of {rev}")
    killed = 0
    for name, path, old, new, runs in MUTANTS:
        file = os.path.join(clone, path)
        text = open(file).read()
        if text.count(old) != 1:
            print(f"!! {name}: the edit does not apply ({text.count(old)} matches)")
            continue
        open(file, "w").write(text.replace(old, new))
        why = []
        for args in runs:
            code, failed, crashed, build = run(clone, args)
            if build:
                why.append(f"{' '.join(args)}: does not build")
            elif crashed:
                why.append(f"{' '.join(args)}: crashed ({crashed})")
            elif code != 0:
                why.append(f"{' '.join(args)}: " + ", ".join(failed[:4]) + (f" (+{len(failed) - 4})" if len(failed) > 4 else ""))
            if why:
                break
        open(file, "w").write(text)
        # A mutant that does not build was not seen by a test: no kill.
        unbuilt = any(w.endswith("does not build") for w in why)
        killed += bool(why) and not unbuilt
        verdict = "UNBUILT" if unbuilt else "KILLED" if why else "SURVIVED"
        print(f"{verdict} {name}: {'; '.join(why) or 'every test passed'}")
    print(f"== {killed}/{len(MUTANTS)} killed")


if __name__ == "__main__":
    main()
