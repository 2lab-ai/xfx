
#[cfg(test)]
mod ctrlo_wider_cluster_dump {
    /// Clusters that hold one of the 14 fillers or an FX_COUNTED scalar next to
    /// emoji sequences, joiners and marks: `codes width squeezed` per line.
    #[test]
    #[ignore = "a receipt dump"]
    fn cluster_dump() {
        let out = std::env::var("XFX_CLUSTER_DUMP").expect("XFX_CLUSTER_DUMP");
        let owners: Vec<u32> = [0x00ad, 0x115f, 0x1160, 0x3164, 0xffa0, 0xfff0, 0xfff8, 0x0605, 0x09be, 0x302e, 0xff9e, 0x1d165, 0x1f1e6, 0x1f1fa, 0xe0000, 0xe0080].to_vec();
        let others: &[&str] = &[
            "\u{1f44d}", "\u{1f44d}\u{1f3fd}", "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", "\u{2764}\u{fe0f}",
            "\u{1f1fa}\u{1f1f8}", "\u{1f1e6}", "\u{1f3fd}", "\u{200d}", "\u{200d}\u{1f44d}", "\u{200d}\u{1f44d}\u{1f3fd}",
            "\u{301}", "\u{fe0f}", "\u{fe0e}", "\u{20e3}", "\u{1160}", "\u{1161}", "a", "\u{995}", "\u{9be}", "\u{302e}",
            "\u{e0067}\u{e0062}\u{e007f}", "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
        ];
        let mut lines = String::new();
        let mut seen = std::collections::BTreeSet::new();
        for &owner in &owners {
            let o = char::from_u32(owner).unwrap().to_string();
            for other in others {
                for text in [format!("{o}{other}"), format!("{other}{o}"), format!("{o}{other}{other}")] {
                    let mut rest = text.as_str();
                    while let Some(cluster) = super::super::wrap::first_cluster(rest) {
                        rest = &rest[cluster.len()..];
                        if !seen.insert(cluster.to_string()) {
                            continue;
                        }
                        let codes: Vec<String> = cluster.chars().map(|c| format!("{:04X}", u32::from(c))).collect();
                        lines.push_str(&format!("{} {} {}\n", codes.join("+"), super::super::wrap::cluster_width(cluster), super::squeezed(cluster)));
                    }
                }
            }
        }
        std::fs::write(out, lines).expect("the dump");
    }
}
