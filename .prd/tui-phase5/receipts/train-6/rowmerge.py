"""Resolve docs/parity.md's conflicted rows clause by clause: each conflicted
row is merged word by word (git merge-file over one token per line) against
the merge base's row of the same name; a word-level conflict stops the script.
usage: rowmerge.py <base-rev> <ours-rev> <theirs-rev>  (run in the worktree)"""
import subprocess, sys, tempfile, os
base_rev, ours_rev, theirs_rev = sys.argv[1:4]
def rows(rev):
    t = subprocess.run(['git', 'show', f'{rev}:docs/parity.md'], capture_output=True, text=True, check=True).stdout
    return {l.split('|')[1].strip(): l for l in t.split('\n') if l.startswith('| ') and l.count('|') > 3}
B, O, T = rows(base_rev), rows(ours_rev), rows(theirs_rev)
def merge_row(name):
    b, o, t = B.get(name), O[name], T[name]
    if o == t: return o
    if b == o: return t
    if b == t: return o
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for tag, x in (('o', o), ('b', b), ('t', t)):
            p = os.path.join(d, tag); open(p, 'w').write('\n'.join(x.split(' ')) + '\n'); paths.append(p)
        r = subprocess.run(['git', 'merge-file', '-p', paths[0], paths[1], paths[2]], capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f'word-level conflict in row {name!r}')
        return ' '.join(r.stdout.rstrip('\n').split('\n'))
lines = open('docs/parity.md', encoding='utf-8').read().split('\n')
out = []; i = 0; n = 0
while i < len(lines):
    if lines[i].startswith('<<<<<<< '):
        j = i + 1; ours = []
        while not lines[j].startswith('======='): ours.append(lines[j]); j += 1
        k = j + 1; theirs = []
        while not lines[k].startswith('>>>>>>> '): theirs.append(lines[k]); k += 1
        names_o = [l.split('|')[1].strip() for l in ours]; names_t = [l.split('|')[1].strip() for l in theirs]
        if names_o != names_t:
            sys.exit(f'conflict rows differ in name/order: {names_o} vs {names_t}')
        for name in names_o:
            out.append(merge_row(name)); n += 1
            print('merged row:', name, '(base==ours %s, base==theirs %s)' % (B.get(name) == O[name], B.get(name) == T[name]))
        i = k + 1
    else:
        out.append(lines[i]); i += 1
open('docs/parity.md', 'w', encoding='utf-8').write('\n'.join(out))
print('rows merged', n)
