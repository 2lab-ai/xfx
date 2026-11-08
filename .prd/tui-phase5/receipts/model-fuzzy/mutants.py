"""MODEL-FUZZY mutants, each on a throwaway clone (git clone --no-local), each
test run under a time limit. usage: python3 -I mutants.py <clone> <target-dir> <out>"""
import os, subprocess, sys, time
clone, target, out = sys.argv[1:4]
MS = os.path.join(clone, 'src/tui/model_search.rs')
MM = os.path.join(clone, 'src/tui/model_menu.rs')
MC = os.path.join(clone, 'src/tui/model_column.rs')
M = [
 ('tier-order: no ranking (catalog order)', MS,
  'found.sort_by(|(left, _), (right, _)| right.cmp(left));', ''),
 ('tier-order: words above substring', MS,
  '    Words,\n    /// The whole query appears in the id as typed, ignoring case.\n    Substring,\n}',
  '    /// The whole query appears in the id as typed, ignoring case.\n    Substring,\n    Words,\n}'),
 ('word-start: any letter starts a word', MS,
  '!previous.is_ascii_alphanumeric() || previous.is_ascii_digit() != byte.is_ascii_digit()', 'true || previous == 0'),
 ('word-start: a case change starts a word', MS,
  '!previous.is_ascii_alphanumeric() || previous.is_ascii_digit() != byte.is_ascii_digit()',
  '!previous.is_ascii_alphanumeric() || previous.is_ascii_digit() != byte.is_ascii_digit() || (previous.is_ascii_lowercase() && byte.is_ascii_uppercase())'),
 ('word-start: no letter/digit turn', MS,
  '!previous.is_ascii_alphanumeric() || previous.is_ascii_digit() != byte.is_ascii_digit()', '!previous.is_ascii_alphanumeric()'),
 ('all-words: a word that matches nothing is passed over', MS,
  '        } else {\n            return Match::None;\n        }', '        } else {\n            continue;\n        }'),
 ('id-only: the model part after the slash only', MS,
  '    let id = id.as_bytes();', "    let id = id.split_once('/').map_or(id, |(_, model)| model).as_bytes();"),
 ('id-only: the column also searches names and aliases', MC,
  '        let mut models = model_search::ranked(catalog.rows(), |entry| entry.id.as_str(), query);\n',
  '        let mut models = model_search::ranked(catalog.rows(), |entry| entry.id.as_str(), query);\n'
  '        for row in catalog.rows() {\n'
  '            let hit = |text: &str| model_search::of(text, query) != model_search::Match::None;\n'
  '            if !models.iter().any(|m| std::ptr::eq(*m, row)) && (row.aliases.iter().any(|a| hit(a)) || row.name.as_deref().is_some_and(hit)) {\n'
  '                models.push(row);\n            }\n        }\n'),
 ('id-only: the menu also searches the vendor apart (expected EQUIVALENT)', MM,
  "            &self.query,\n        )\n    }",
  "            &self.query,\n        )\n        .into_iter()\n"
  "        .chain(self.items.iter().filter(|item| {\n"
  "            let query = self.query.trim_matches([' ', '\\t', '\\r', '\\n']).to_ascii_lowercase();\n"
  "            let vendor = item.id.split_once('/').map_or(\"\", |(vendor, _)| vendor).to_ascii_lowercase();\n"
  "            (self.tab == Vendor::All || item.tab == self.tab) && !query.is_empty() && vendor.contains(&query)\n"
  "                && model_search::of(&item.id, &self.query) == model_search::Match::None\n"
  "        }))\n        .collect()\n    }"),
 ('query: the blanks round it are kept', MS,
  '    let text = query.trim_matches(BLANKS);', '    let text = query;'),
 ('column: the cap before the ranks', MC,
  'model_search::ranked(catalog.rows(), |entry| entry.id.as_str(), query);',
  'model_search::ranked(catalog.rows().iter().take(MOST_MODELS), |entry| entry.id.as_str(), query);'),
 ('column: Tab keeps the mark where it was', MC,
  '                self.model_index = 0;\n                self.model_window = 0;\n                Some(format!("{PREFIX}{id}"))',
  '                Some(format!("{PREFIX}{id}"))'),
 ('menu: the mark looked up in catalog order on a revalidation', MM,
  '.and_then(|id| items.iter().position(|item| item.id == id))',
  '.and_then(|id| self.items.iter().position(|item| item.id == id))'),
]
env = dict(os.environ, CARGO_TARGET_DIR=target)
for k in ('GIT_DIR', 'GIT_INDEX_FILE', 'GIT_WORK_TREE'):
    env.pop(k, None)
cmd = ['timeout', '900', 'cargo', 'test', '--locked', '--lib', '--', 'tui::model_menu', 'tui::model_column', 'tui::model_search']
with open(out, 'w') as log:
    def run(label):
        t0 = time.time()
        p = subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True)
        text = p.stdout + p.stderr
        failed = [l.split(' ')[1] for l in text.splitlines() if l.startswith('test ') and l.endswith('FAILED')]
        if p.returncode == 124: verdict = 'TIMEOUT'
        elif 'error[' in text or 'could not compile' in text: verdict = 'BUILD-ERROR'
        elif p.returncode == 0: verdict = 'SURVIVED'
        else: verdict = 'KILLED'
        summary = [l for l in text.splitlines() if l.startswith('test result')]
        log.write(f'## {label}\nverdict: {verdict} (exit {p.returncode}, {time.time()-t0:.0f} s)\n')
        log.write(''.join(f'  failed: {f}\n' for f in failed))
        log.write(''.join(f'  {s}\n' for s in summary))
        if verdict == 'BUILD-ERROR':
            log.write('\n'.join('  ' + l for l in text.splitlines() if 'error' in l)[:3000] + '\n')
        log.flush()
        return verdict
    subprocess.run(['git', '-C', clone, 'checkout', '-q', '--', '.'], env=env, check=True)
    run('control: the unit as committed')
    for label, path, old, new in M:
        subprocess.run(['git', '-C', clone, 'checkout', '-q', '--', '.'], env=env, check=True)
        src = open(path).read()
        n = src.count(old)
        if n != 1:
            log.write(f'## {label}\nverdict: NOT-APPLIED (pattern found {n} times)\n'); log.flush(); continue
        open(path, 'w').write(src.replace(old, new))
        run(label)
    subprocess.run(['git', '-C', clone, 'checkout', '-q', '--', '.'], env=env, check=True)
