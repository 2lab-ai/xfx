"""Adds a test, never committed, to a file_index.rs: it searches fx's
100,000-entry inventory (`fx_inventory`) for every prefix of fx's eight
benchmark queries -- the 73 prefixes the cost test times -- and writes each
row to $ROWDIFF_OUT, one line a row: the prefix, the row's place, its kind,
its path and its bold spans. A prefix with no row writes one line saying so.
usage: python3 -I r4-rowdump.py <file_index.rs>   (then run the test
`tui::file_index::tests::zz_rows_dump` with `--ignored`)"""

import sys

p = sys.argv[1]
s = open(p, encoding="utf-8").read()
anchor = "    #[test]\n    fn a_path_shaped_query_browses_rather_than_searches() {\n"
assert s.count(anchor) == 1, "no anchor"
dump = '''    #[test]
    #[ignore = "ROW DUMP, not to be committed"]
    fn zz_rows_dump() {
        let entries = fx_inventory(MOST_INDEXED);
        let queries = [
            "src/core/workspace/file_index.zig",
            "scwfi",
            "ae",
            "ashr",
            "zzz-no-match-\\u{3a9}",
            "api-route",
            "\\u{e4}rf",
            "Klv",
        ];
        let mut out = String::new();
        let mut prefixes = 0;
        for query in queries {
            for end in query.char_indices().skip(1).map(|(end, _)| end).chain([query.len()]) {
                let prefix = &query[..end];
                prefixes += 1;
                let found = search(&entries, prefix);
                if found.is_empty() {
                    out += &format!("{prefix:?}\\t-\\tno rows\\n");
                }
                for (at, row) in found.iter().enumerate() {
                    out += &format!("{prefix:?}\\t{at}\\t{:?}\\t{}\\t{:?}\\n", row.kind, row.path, row.spans);
                }
            }
        }
        assert_eq!(prefixes, 73);
        std::fs::write(std::env::var("ROWDIFF_OUT").expect("ROWDIFF_OUT"), out).expect("written");
    }

'''
s = s.replace(anchor, dump + anchor)
open(p, "w", encoding="utf-8").write(s)
