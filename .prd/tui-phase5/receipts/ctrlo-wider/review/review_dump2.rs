
#[cfg(test)]
mod ctrlo_wider_review_dump2 {
    /// Every pair of regional indicators between two letters through the
    /// review's three spellings, `first second tamed named command`.
    #[test]
    #[ignore = "a receipt dump"]
    fn review_dump_pairs() {
        let out = std::env::var("XFX_REVIEW_DUMP").expect("XFX_REVIEW_DUMP");
        let mut lines = String::new();
        for first in 0x1f1e6..=0x1f1ffu32 {
            for second in 0x1f1e6..=0x1f1ffu32 {
                let text = format!("a{}{}b", char::from_u32(first).unwrap(), char::from_u32(second).unwrap());
                let forms = [super::tamed(&text), super::named(&text), super::command_text(&text)];
                let spell = |form: &String| if *form == text { "=".to_string() } else { form.escape_default().to_string() };
                lines.push_str(&format!("{first:x} {second:x} {} {} {}\n", spell(&forms[0]), spell(&forms[1]), spell(&forms[2])));
            }
        }
        std::fs::write(out, lines).expect("the dump");
    }
}
