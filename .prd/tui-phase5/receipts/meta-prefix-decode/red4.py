"""RED for round 4's three pins: each pin as its own test (appended to the
test module of a throwaway clone), run under its forced mutant, then with the
mutant reverted. usage: TMPDIR=<tmp> python3 -I red4.py <clone> <out>"""
import os, subprocess, sys
repo, outp = sys.argv[1], sys.argv[2]
assert '/mpd/red4/' in os.path.realpath(repo)
I = os.path.join(repo, 'src/tui/input.rs')
orig = open(I, encoding='utf-8').read()
HELPER = '''
    fn red4_run(schedule: &[(u64, &[u8])]) -> Vec<Input> {
        let start = Instant::now();
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        for (offset, bytes) in schedule {
            let now = start + Duration::from_millis(*offset);
            decoder.flush(now, &mut out);
            for byte in *bytes {
                decoder.feed(*byte, now, &mut out);
            }
        }
        decoder.flush(start + Duration::from_millis(1000), &mut out);
        out
    }

    #[test]
    fn red4_ctrl_c_at_exactly_30_ms() {
        let escape = || Input::Action(Action::Escape);
        assert_eq!(
            red4_run(&[(0, b"\\x1b\\x1b"), (30, &[0x03])]),
            vec![escape(), escape(), Input::Action(Action::Cancel)]
        );
    }

    #[test]
    fn red4_a_third_esc_restarts_the_pairs_clock() {
        assert_eq!(
            red4_run(&[(0, b"\\x1b\\x1b"), (20, b"\\x1b"), (40, b"[A")]),
            vec![Input::Action(Action::ParagraphUp)]
        );
    }

    #[test]
    fn red4_the_kept_escs_expiry_staggered() {
        let start = Instant::now();
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.feed(0x1b, start, &mut out);
        decoder.feed(0x1b, start + Duration::from_millis(20), &mut out);
        for (at, count) in [(49, 0), (50, 1), (69, 1), (70, 2), (1000, 2)] {
            decoder.flush(start + Duration::from_millis(at), &mut out);
            assert_eq!(out.len(), count, "at {at} ms: {out:?}");
        }
    }
}
'''
assert orig.endswith('}\n')
with_tests = orig[:-2] + HELPER
PAIR_IF = "        if meta && quiet >= Self::PAIR_TIMEOUT {\n"
KEEP = "            self.stage = Stage::Esc { at, meta: false };\n"
SECOND = "            0x1b => {\n                self.stage = Stage::Esc {\n                    at: now,\n                    meta: true,\n"
CASES = [
    ("red4_ctrl_c_at_exactly_30_ms", "the pair's stage strict (quiet > PAIR_TIMEOUT)", PAIR_IF, "        if meta && quiet > Self::PAIR_TIMEOUT {\n"),
    ("red4_a_third_esc_restarts_the_pairs_clock", "a third ESC keeps the pair's clock (0x1b if meta => {})", SECOND, "            0x1b if meta => {}\n" + SECOND),
    ("red4_the_kept_escs_expiry_staggered", "the kept ESC's clock restarts at the pair's stage (at: now)", KEEP, "            self.stage = Stage::Esc { at: now, meta: false };\n"),
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(os.path.dirname(repo), 'target'))
lines = []
def run(name):
    r = subprocess.run(['cargo', 'test', '--locked', '--lib', '--', name, '--exact'], cwd=repo, env=env, capture_output=True, text=True, timeout=1500)
    keep = [l for l in r.stdout.splitlines() if l.startswith('test ') or l.startswith('test result') or 'left:' in l or 'right:' in l or 'at 70 ms' in l or 'panicked' in l]
    return [l.split(' (')[0] if 'panicked' in l else l for l in keep]
try:
    for name, label, old, new in CASES:
        assert with_tests.count(old) == 1, (name, with_tests.count(old))
        open(I, 'w', encoding='utf-8').write(with_tests.replace(old, new))
        lines.append(f'## {name} under the forced mutant: {label}')
        lines += run('tui::input::tests::' + name)
        open(I, 'w', encoding='utf-8').write(with_tests)
        lines.append(f'## {name} at the head (the mutant reverted)')
        lines += run('tui::input::tests::' + name)
        lines.append('')
finally:
    open(I, 'w', encoding='utf-8').write(orig)
st = subprocess.run(['git', '-C', repo, 'status', '--porcelain'], capture_output=True, text=True).stdout
lines.append("the clone's tree after the run: " + ('clean' if not st.strip() else 'DIRTY'))
open(outp, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
print('\n'.join(lines))
