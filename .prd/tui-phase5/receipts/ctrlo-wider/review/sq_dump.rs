
#[cfg(test)]
mod ctrlo_wider_squeezed_dump {
    #[test]
    #[ignore = "a receipt dump"]
    fn squeezed_dump() {
        let out = std::env::var("XFX_SQ_DUMP").expect("XFX_SQ_DUMP");
        let mut lines = String::new();
        for text in ["x\u{1f44d}\u{1f3fd}\u{302e}y", "\u{1f44d}\u{1f3fd}\u{302e}", "x\u{1f44d}\u{302e}y", "x\u{995}\u{9be}y", "x\u{1f1e6}\u{302e}y"] {
            lines.push_str(&format!(
                "{} | tamed {} | named {} | command {}\n",
                text.escape_unicode(),
                super::tamed(text).escape_default(),
                super::named(text).escape_default(),
                super::command_text(text).escape_default()
            ));
        }
        std::fs::write(out, lines).expect("the dump");
    }
}
