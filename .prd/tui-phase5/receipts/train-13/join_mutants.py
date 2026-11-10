#!/usr/bin/env python3
"""Train 13: the two join tests' teeth -- each unit change a join test rests
on, taken out of a throwaway clone (jobs.sh's `mutant` part restores the
clone after each). Each edit must apply exactly once, or nothing is written.

  j1-history    HISTORY-NOTICES-COPY-FX's deleted lines back, in short: a
                state left behind pushes a notice (`src/session/store.rs`,
                `turn_messages`, after the line that picks the state)
  j1-resume     QUESTION-RESUME's decoder never decodes: `decode_answers`
                returns None (`src/tools/question.rs`)
  j2-word-left  QUESTION-WORD-LEFT's Alt-b read as the plain word left by
                the question panel, as before the unit (`Act::of`,
                `src/tui/question.rs`)

    python3 -I -B join_mutants.py <clone> <mutant>
"""
import sys

clone, name = sys.argv[1:3]
EDITS = {
    "j1-history": ("src/session/store.rs",
                   "                let state: &[serde_json::Value] = if same_model { on_wire } else { &[] };\n",
                   "                let state: &[serde_json::Value] = if same_model { on_wire } else { &[] };\n"
                   "                if state.is_empty() && !(raw_content.is_empty() && responses_state.is_empty()) {\n"
                   "                    notices.push(\"xfx: this session recorded reasoning that was not carried over\".to_string());\n"
                   "                }\n"),
    "j1-resume": ("src/tools/question.rs",
                  "pub fn decode_answers(output: &str) -> Option<Vec<SavedAnswer>> {\n",
                  "pub fn decode_answers(output: &str) -> Option<Vec<SavedAnswer>> {\n    if !output.is_empty() {\n        return None;\n    }\n"),
    "j2-word-left": ("src/tui/question.rs",
                     "            Action::AltB => Some(Self::AltB),\n",
                     "            Action::AltB => Some(Self::WordLeft),\n"),
}
path, old, new = EDITS[name]
p = f"{clone}/{path}"
s = open(p, encoding="utf-8").read()
if s.count(old) != 1:
    sys.exit(f"{name}: {path} holds the line {s.count(old)} times")
open(p, "w", encoding="utf-8").write(s.replace(old, new))
print(f"{name}: {path} edited")
