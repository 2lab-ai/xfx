
#[cfg(test)]
mod ctrlo_wider_review_dump {
    /// Every scalar between two letters through the review's three
    /// spellings: one line per scalar any of them changes, `hex tamed named
    /// command` with each `=` when it kept the text, to `XFX_REVIEW_DUMP`.
    #[test]
    #[ignore = "a receipt dump"]
    fn review_dump() {
        let out = std::env::var("XFX_REVIEW_DUMP").expect("XFX_REVIEW_DUMP");
        let mut lines = String::new();
        for code in 0..0x11_0000u32 {
            if code < 0x20 || (0x7f..=0x9f).contains(&code) {
                continue;
            }
            let Some(c) = char::from_u32(code) else { continue };
            let text = format!("a{c}b");
            let forms = [super::tamed(&text), super::named(&text), super::command_text(&text)];
            if forms.iter().all(|form| *form == text) {
                continue;
            }
            let spell = |form: &String| if *form == text { "=".to_string() } else { form.escape_default().to_string() };
            lines.push_str(&format!("{code:x} {} {} {}\n", spell(&forms[0]), spell(&forms[1]), spell(&forms[2])));
        }
        std::fs::write(out, lines).expect("the dump");
    }
}
