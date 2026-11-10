"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the decoder's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
DECODER-TILDE-HOME-END's mutants.py, with this unit's mutants: one per
spelling class that changed (M1-M9), then the first field's digit (M10) and
the paste markers kept off the numeric path (M11); round 2 adds a wrap in the
key field alone (M12) and in the keycode field alone (M13), which the
modifier's tests cannot see."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
MODIFIERS = "    let modifiers = modifier.saturating_sub(1);\n"
FIELD = "    let field = |field: Option<&[u8]>| field.map_or(Some(0), tilde_field);\n"
TILDE_FIELD = "fn tilde_field(field: &[u8]) -> Option<u16> {\n"
DELETE = "        3 => forward_delete(modifiers),\n"
REPORT = "    if code.is_some() {\n"
FIRST = "    if key.is_empty() {\n        return ignore;\n    }\n"
OTHER = "        _ => Action::Ignore,\n    })\n}\n"
CALL = "        (tilde_field(key), field(modifier), field(code))\n"
WRAP = "{0}.iter().try_fold(0u16, |v, b| b.is_ascii_digit().then(|| v.wrapping_mul(10).wrapping_add(u16::from(b - b'0'))))"
MUTANTS = [
    ("M1 a modifier field of 1 is no key (the old `mask == 0` guard)", I, MODIFIERS,
     "    if modifier == 1 && code.is_none() {\n        return ignore;\n    }\n" + MODIFIERS),
    ("M2 an empty field is no field", I, FIELD,
     "    let field = |field: Option<&[u8]>| {\n"
     "        field.map_or(Some(0), |f| if f.is_empty() { None } else { tilde_field(f) })\n"
     "    };\n"),
    ("M3 a field of 0 is no field", I, FIELD,
     "    let field = |field: Option<&[u8]>| {\n"
     "        field.map_or(Some(0), |f| if f == b\"0\" { None } else { tilde_field(f) })\n"
     "    };\n"),
    ("M4 a leading zero is no number (the old canonical spelling)", I, TILDE_FIELD,
     TILDE_FIELD + "    if field.len() > 1 && field[0] == b'0' {\n        return None;\n    }\n"),
    ("M5 a modifier field past 256 is no key (the old eight bits)", I, MODIFIERS,
     "    if modifier > 256 {\n        return ignore;\n    }\n" + MODIFIERS),
    ("M6 a field past 65535 wraps instead of saturating", I,
     "                .saturating_mul(10)\n                .saturating_add(u16::from(byte - b'0'))\n",
     "                .wrapping_mul(10)\n                .wrapping_add(u16::from(byte - b'0'))\n"),
    ("M7 Delete reads its field with the lock bits masked off (the old reading)", I, DELETE,
     "        3 => forward_delete(modifiers & 0x3f),\n"),
    ("M8 Delete reads only the field's low byte", I, DELETE,
     "        3 => forward_delete(u16::from(low)),\n"),
    ("M9 a third field is a key report only under 27 (the old family)", I, REPORT,
     REPORT + "        if key != 27 {\n            return ignore;\n        }\n"),
    ("M10 an empty first field is read as 0", I, FIRST, ""),
    ("M11 the number 200 opens a paste (the markers on the numeric path)", I, OTHER,
     "        200 => Action::PasteStart,\n" + OTHER),
    # Round 2: a wrap in one field at a time, the others still saturating.
    ("M12 the key field wraps past 65535", I, CALL,
     CALL.replace("tilde_field(key)", WRAP.format("key"))),
    ("M13 a key report's keycode field wraps past 65535", I, CALL,
     CALL.replace("field(code)", "code.map_or(Some(0), |code| " + WRAP.format("code") + ")")),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "tui::input::tests"],
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, path, old, new in MUTANTS:
    full = os.path.join(repo, path)
    src = open(full).read()
    if src.count(old) != 1:
        say(f"{name}: NOT APPLIED (pattern found {src.count(old)} times)"); continue
    open(full, "w").write(src.replace(old, new))
    verdict, why = "SURVIVED", ""
    t0 = time.time()
    try:
        for cmd in TESTS:
            try:
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=900)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", ", ".join(f.removeprefix("tui::input::tests::") for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
