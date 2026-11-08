"""Resolve docs/parity.md's conflicted rows clause by clause: each conflicted
row is merged word by word (git merge-file over one token per line) against
the merge base's row of the same name; a word-level conflict stops the script.
Train 6's rowmerge.py; train 7 adds `--sub <side>:<old>=<new>`: a
reference number a side re-pointed on its own branch (one whitespace token,
exact), set to the line the same code is on in the merged tree before the
word merge, so two sides' re-points of one reference agree.
usage: rowmerge.py <base-rev> <ours-rev> <theirs-rev> [--sub ours|theirs:<old>=<new>]...
(run in the worktree)"""
import subprocess, sys, tempfile, os
base_rev, ours_rev, theirs_rev = sys.argv[1:4]
subs = {'ours': [], 'theirs': []}
args = sys.argv[4:]
while args:
    flag, spec = args[0], args[1]; args = args[2:]
    assert flag == '--sub'
    side, pair = spec.split(':', 1)
    subs[side].append(pair.split('=', 1))
def rows(rev):
    t = subprocess.run(['git', 'show', f'{rev}:docs/parity.md'], capture_output=True, text=True, check=True).stdout
    return {l.split('|')[1].strip(): l for l in t.split('\n') if l.startswith('| ') and l.count('|') > 3}
def subbed(side, row):
    toks = row.split(' ')
    for old, new in subs[side]:
        n = toks.count(old)
        if n:
            print(f'  {side}: {old!r} -> {new!r} ({n}x)')
        toks = [new if t == old else t for t in toks]
    return ' '.join(toks)
import difflib
def within(b, mine, other):
    """Every edit base->mine (difflib opcode: base span, new tokens) is an
    edit base->other makes too, at the same base span."""
    def edits(x):
        return {(i1, i2): tuple(x[j1:j2]) for op, i1, i2, j1, j2 in
                difflib.SequenceMatcher(None, b, x, autojunk=False).get_opcodes() if op != 'equal'}
    b, mine, other = b.split(' '), mine.split(' '), other.split(' ')
    theirs = edits(other)
    return all(theirs.get(span) == new for span, new in edits(mine).items())
B, O, T = rows(base_rev), rows(ours_rev), rows(theirs_rev)
def merge_row(name):
    b, o, t = B.get(name), subbed('ours', O[name]), subbed('theirs', T[name])
    if o == t: return o
    if b == o: return t
    if b == t: return o
    # Train 7: a side whose every change since the base (after --sub) the
    # other side made too, token for token at the same place, adds nothing
    # of its own: the row is the other side's.
    for mine, other, tag in ((o, t, 'ours'), (t, o, 'theirs')):
        if within(b, mine, other):
            print(f'  every change {tag} made, the other side made too: the other side\'s row')
            return other
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
            print('row:', name)
            out.append(merge_row(name)); n += 1
            print('  merged (base==ours %s, base==theirs %s)' % (B.get(name) == O[name], B.get(name) == T[name]))
        i = k + 1
    else:
        out.append(lines[i]); i += 1
open('docs/parity.md', 'w', encoding='utf-8').write('\n'.join(out))
print('rows merged', n)
