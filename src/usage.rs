//! `/usage`: what the provider says this account has spent.
//!
//! fx v0.0.16 reads `/usage` from the provider's own usage reports and keeps
//! no usage ledger of its own (`vercel-labs/fx@a41fc2d4` *Read /usage from
//! provider usage reports*): the report client ([`report`]), the shared cache
//! of its answers ([`cache`]), the periods it is cut into ([`view`]), when it
//! is asked ([`policy`]) and the runtime that asks ([`dashboard`]). The TUI's
//! screen over it is `crate::tui::usage_menu`.
//!
//! **The local ledger is retired** (US-U5), as upstream's was in the same
//! commit: no turn records what its requests cost, no bill is asked for by
//! generation, and `~/.xfx/usage.json` -- when counting began -- is neither
//! written nor read. A session or a profile an older build left them in is
//! left as it is, its old fields read past ([`crate::session::SessionEvent::UsageRecorded`]).
//! What stays here is how a number is written.

pub mod cache;
pub mod clock;
pub mod dashboard;
pub mod policy;
pub mod report;
pub mod view;

// ---------------------------------------------------------------------------
// numbers, written the way upstream writes them
// ---------------------------------------------------------------------------

/// `value` with `places` decimals, rounded the way upstream's `{d:.N}` rounds:
/// on the **shortest** decimal that reads back as the same number, half away
/// from zero.
///
/// Not Rust's `{:.N}`, which rounds the exact binary value to even: the two
/// disagree on a tie, and a tie is what a price is made of -- upstream writes
/// `0.125` as `0.13` and `1.25` as `1.3`, where Rust writes `0.12` and `1.2`.
pub fn fixed(value: f64, places: usize) -> String {
    if !value.is_finite() {
        return "?".to_string();
    }
    let negative = value < 0.0;
    // `Display` is the shortest round-trip decimal, and never an exponent.
    let shortest = format!("{}", value.abs());
    let (whole, fraction) = shortest.split_once('.').unwrap_or((&shortest, ""));
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).collect();
    let mut point = whole.len();
    if fraction.len() > places {
        let keep = point + places;
        let round_up = digits[keep] >= b'5';
        digits.truncate(keep);
        if round_up {
            let mut at = digits.len();
            loop {
                if at == 0 {
                    digits.insert(0, b'1');
                    point += 1;
                    break;
                }
                at -= 1;
                if digits[at] == b'9' {
                    digits[at] = b'0';
                } else {
                    digits[at] += 1;
                    break;
                }
            }
        }
    } else {
        digits.extend(std::iter::repeat_n(b'0', places - fraction.len()));
    }
    let text = String::from_utf8(digits).unwrap_or_default();
    let (whole, fraction) = text.split_at(point);
    let sign = if negative { "-" } else { "" };
    if places == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

/// A count with its thousands marked: `1,234` (`formatGroupedUnsigned`).
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_with_upstreams_rounding_not_rusts() {
        // The ties Rust's exact rounding and upstream's shortest-digit
        // rounding disagree on, measured against the Zig 0.16 build of fx.
        assert_eq!(fixed(0.125, 2), "0.13");
        assert_eq!(fixed(0.015, 2), "0.02");
        assert_eq!(fixed(1.005, 2), "1.01");
        assert_eq!(fixed(2.675, 2), "2.68");
        assert_eq!(fixed(1.25, 1), "1.3");
        assert_eq!(fixed(0.15, 1), "0.2");
        // And the ones they agree on, from the capture.
        assert_eq!(fixed(0.0125, 2), "0.01");
        assert_eq!(fixed(0.4335, 2), "0.43");
        assert_eq!(fixed(2.5, 2), "2.50");
        assert_eq!(fixed(0.0, 2), "0.00");
        assert_eq!(fixed(9.96, 1), "10.0");
        assert_eq!(fixed(99.99, 1), "100.0");
    }

    #[test]
    fn a_count_has_its_thousands_marked() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(123), "123");
    }
}
