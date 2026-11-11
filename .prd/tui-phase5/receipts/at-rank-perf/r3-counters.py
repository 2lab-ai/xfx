"""Temporary counters for the profiling harness (never committed): how often
align_best runs and how far its scans and isSubsequence read, per search,
and how often the cuts are on. usage: python3 -I counters.py <file_index.rs>"""
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
def rep(a, b):
    global s
    assert s.count(a) == 1, a[:60]
    s = s.replace(a, b)
s = s.replace("use std::cmp::Ordering;\n", "use std::cmp::Ordering;\npub(crate) static PC: [std::sync::atomic::AtomicUsize; 16] = [const { std::sync::atomic::AtomicUsize::new(0) }; 16];\nfn pc(i: usize, n: usize) { PC[i].fetch_add(n, std::sync::atomic::Ordering::Relaxed); }\n", 1)
rep("""    let m = query.len();
    if m == 0 || range_start > text.len() || m > text.len() - range_start {""", """    let m = query.len();
    pc(0, 1);
    if m == 0 || range_start > text.len() || m > text.len() - range_start {""")
rep("""    if s.forward.len() < m {
        return None;
    }""", """    pc(2, text.len() - range_start);
    if s.forward.len() < m {
        pc(3, 1);
        return None;
    }""")
rep("""    s.row_offsets.push(s.columns.len());
    let slots = s.columns.len() * 4;""", """    s.row_offsets.push(s.columns.len());
    pc(6, 1); pc(8, s.columns.len());
    let slots = s.columns.len() * 4;""")
rep("""fn is_subsequence<T: PartialEq>(text: &[T], query: &[T]) -> bool {
""", """fn is_subsequence<T: PartialEq>(text: &[T], query: &[T]) -> bool {
    pc(9, 1); pc(10, text.len());
""")
rep("""        let Some(score) = score_entry(entry, query, needed, limit, s) else {""", """        if !needed.folder { pc(12, 1); }
        if !needed.crossing { pc(13, 1); }
        let Some(score) = score_entry(entry, query, needed, limit, s) else {""")
rep("""    if best.is_none() && query.len() > 1 && !is_subsequence(text, query) {""",
    """    if best.is_none() && query.len() > 1 && !needed.folder { pc(14, 1); }
    if best.is_none() && query.len() > 1 && !is_subsequence(text, query) {""")
rep("""        eprintln!("PROFILE DONE {n} searches, {:?} each", start.elapsed() / n as u32);""",
"""        eprintln!("PROFILE DONE {n} searches, {:?} each", start.elapsed() / n as u32);
        for (i, name) in [(0, "align_best runs"), (2, "align_best greedy scan, bytes from its start"), (3, "align_best, no placement in its range"), (6, "align_best dynamic programs"), (8, "align_best cells"), (9, "isSubsequence runs"), (10, "isSubsequence, bytes in its path"), (12, "entries with the folder cut on"), (13, "entries with the crossing cut on"), (14, "isSubsequence runs with the folder cut on")] {
            eprintln!("PC {name:44} per search {:>9}", PC[i].load(std::sync::atomic::Ordering::Relaxed) / n);
        }""")
open(p, "w", encoding="utf-8").write(s)
