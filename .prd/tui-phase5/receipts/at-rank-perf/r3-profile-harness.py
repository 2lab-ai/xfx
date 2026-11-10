"""Adds the profiling harness (a test, never committed) to a file_index.rs: it
searches $PROF_Q (comma-separated prefixes) in turn over fx's inventory for
$PROF_S seconds, printing its pid first, so `sample <pid> 8` can profile it.
With a second argument, marks the file_index functions #[inline(never)].
usage: python3 -I r3-profile-harness.py <file_index.rs> [noinline]"""

import sys
p = sys.argv[1]
noinline = len(sys.argv) > 2
s = open(p, encoding="utf-8").read()
anchor = "    #[test]\n    fn a_keystroke_places_no_more_than_its_cuts_allow() {\n"
harness = '''    #[test]
    #[ignore = "PROFILE HARNESS, not to be committed"]
    fn zz_profile_harness() {
        let entries = fx_inventory(MOST_INDEXED);
        let queries: Vec<String> = std::env::var("PROF_Q").unwrap().split(',').map(String::from).collect();
        let secs: u64 = std::env::var("PROF_S").map(|s| s.parse().unwrap()).unwrap_or(10);
        eprintln!("PROFILE READY pid {}", std::process::id());
        let start = Instant::now();
        let mut n = 0usize;
        while start.elapsed() < Duration::from_secs(secs) {
            for q in &queries {
                std::hint::black_box(search(&entries, q));
                n += 1;
            }
        }
        eprintln!("PROFILE DONE {n} searches, {:?} each", start.elapsed() / n as u32);
    }

'''
s = s.replace(anchor, harness + anchor)
if noinline:
    for name in ["fn score_entry(", "fn choose_placement<", "fn is_subsequence<", "fn exact_fit<", "fn admits(", "fn spans(", "fn score(", "fn better(", "fn score_alignment(", "fn rank_top("]:
        i = s.index(name)
        ls = s.rindex("\n", 0, i) + 1
        s = s[:ls] + "#[inline(never)]\n" + s[ls:]
open(p, "w", encoding="utf-8").write(s)
