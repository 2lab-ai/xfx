
    // PASTE-AFTER-END's (c251a591) composer and plain helpers, copied from the head for
    // the join test's run on AT-HEIGHT alone (train 10's jobs.sh alone2): test code only.
    /// The composer's row as plain text: the band's row that starts with
    /// its marker once its colours are taken out, its padding cut.
    fn composer(shell: &Shell) -> String {
        shell
            .band_rows()
            .into_iter()
            .map(|row| plain(&row))
            .find(|row| row.starts_with(COMPOSER_MARKER))
            .map(|row| row.trim_end().to_string())
            .unwrap_or_default()
    }

    /// `row` without its `CSI … m` colour sequences.
    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }
