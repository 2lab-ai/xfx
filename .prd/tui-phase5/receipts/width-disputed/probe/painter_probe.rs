// The painter's column of `clusters.txt`: appended to `src/tui/wrap.rs` on a
// throwaway clone at db3b54a1, never committed there, and run with
//   cargo test --locked --lib painter_probe -- --ignored --nocapture
// Each line of `seqs.txt` and `seqs3.txt` is a form as hex code points; it
// prints `PAINTER <form>|<wrap::width of it>`. The run read copies of the two
// files from scratch; this reads the receipt's.
#[cfg(test)]
mod painter_probe {
    #[test]
    #[ignore]
    fn print_painter_counts() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/.prd/tui-phase5/receipts/width-disputed/probe");
        for file in ["seqs.txt", "seqs3.txt"] {
            for line in std::fs::read_to_string(format!("{dir}/{file}")).unwrap().lines() {
                let text: String = line
                    .split(' ')
                    .map(|c| char::from_u32(u32::from_str_radix(c, 16).unwrap()).unwrap())
                    .collect();
                println!("PAINTER {line}|{}", super::width(&text));
            }
        }
    }
}
