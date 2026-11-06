//! Local wall-clock readings of timestamps, for text a person reads (fx
//! v0.0.16's `src/core/shared/local_time.zig@481104fd`, c9bc4876). xfx keeps
//! and writes times in UTC; a screen reads them in the user's time zone, from
//! the C library's rules for `TZ` or the system's own, at each moment's own
//! offset, so daylight saving time follows the date (`localZoneAt`,
//! `:148-159`).
//!
//! Upstream reads Ctrl-O's entry headers, `fx sessions`, `fx usage` and
//! `fx slack` through it. xfx reads the full transcript's headers
//! (`crate::tui::full_transcript`'s `stamp`); the rest of xfx's times are
//! UTC as their surfaces say, or no clock at all.
//!
//! # What reaches the terminal
//!
//! `TZ` comes from the environment, and with it any zone file a path names
//! (`TZ=/some/file`), whose abbreviations are whatever bytes that file holds.
//! So a reading shows the abbreviation only when it is a short plain token,
//! 1 to 10 of `A-Z a-z 0-9 + -` (`isPlainZoneName`, `:118-124`), and
//! otherwise the offset in the `+HHMM` form tzdata gives a zone that has no
//! name (`writeZoneName`, `:102-116`). An ESC, a CSI or OSC, a C1 control, a
//! byte past ASCII or an eleventh byte never prints. The C library's bytes are
//! copied at most eleven at a time, so a longer name is refused by its length
//! rather than read to its end.
//!
//! An offset of a day or more, or one that would carry the moment out of
//! 1970-9999, reads in UTC instead (`inZone`, `:70-100`). Only the offset and
//! the name are taken from the C library's `struct tm`; the calendar is this
//! module's own arithmetic, so no field the library fills in can make it
//! fail.
//!
//! # Threads
//!
//! `localtime_r` is the re-entrant conversion, but the `tm_zone` it returns
//! points into the C library's own storage, which a later conversion or a
//! `tzset` may overwrite (macOS `localtime(3)`). One lock ([`LIBC_ZONE`]) holds
//! every reading here from the call to the copy of those bytes, so no two of
//! xfx's readings overlap. Nothing else in xfx calls the C library's zone
//! code, and xfx never sets its own environment while it runs: a `setenv`
//! under a reading would race the library's read of `TZ`. `tzset` is called
//! once, under the same lock, before the first conversion: POSIX asks
//! `localtime` to behave as though it had called `tzset` and does not ask it
//! of `localtime_r`, so a portable caller calls it first. Upstream calls none
//! (no `tzset` in its `src/` at 481104fd); macOS's `localtime_r` reads `TZ`
//! without one, so on macOS the call changes nothing.

#[cfg(test)]
use std::cell::Cell;
use std::fmt;

use crate::trace::civil;

/// 9999-12-31 23:59:59.999 UTC, the last moment a four-digit year can show
/// (`max_timestamp_ms`, `:8-9`).
pub(crate) const MAX_TIMESTAMP_MS: i64 = 253_402_300_799_999;

/// The longest abbreviation shown, such as `PDT` or `+0545`
/// (`max_zone_name_len`, `:11-12`).
const MAX_ZONE_NAME_LEN: usize = 10;

/// A fixed offset from UTC and its abbreviation, as the C library (or a test)
/// gives it: bytes, checked only when a reading writes them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Zone<'a> {
    /// Seconds east of UTC.
    pub(crate) offset_s: i32,
    /// The abbreviation, such as `PDT`.
    pub(crate) name: &'a [u8],
}

impl Zone<'static> {
    pub(crate) const UTC: Self = Zone {
        offset_s: 0,
        name: b"UTC",
    };
}

/// A moment's calendar date and clock time in one zone (`Reading`,
/// `:29-60`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) year: i64,
    pub(crate) month: i64,
    /// Day of the month, from 1.
    pub(crate) day: i64,
    pub(crate) hour: i64,
    pub(crate) minute: i64,
    pub(crate) second: i64,
    pub(crate) millisecond: i64,
    /// The abbreviation or the `+HHMM` offset: ASCII, 1 to 10 bytes.
    zone: [u8; MAX_ZONE_NAME_LEN],
    zone_len: usize,
}

impl Reading {
    /// The zone as the reading shows it: a plain abbreviation (`KST`) or the
    /// offset (`+0545`), never anything else.
    pub(crate) fn zone_name(&self) -> &str {
        // Every byte was written by `write_zone_name`, which writes only
        // `A-Z a-z 0-9 + -`, so this is always UTF-8.
        std::str::from_utf8(&self.zone[..self.zone_len]).unwrap_or_default()
    }
}

/// `2026-10-16 13:31:49.123 PDT` (`Reading.format`, `:47-59`).
impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.millisecond,
            self.zone_name()
        )
    }
}

/// Reads `timestamp_ms` in the local zone in effect at that moment, or
/// nothing outside 1970 through 9999 (`local`, `:62-68`).
pub(crate) fn local(timestamp_ms: i64) -> Option<Reading> {
    if !(0..=MAX_TIMESTAMP_MS).contains(&timestamp_ms) {
        return None;
    }
    let mut name = [0; MAX_ZONE_NAME_LEN + 1];
    let zone = zone_at(timestamp_ms.div_euclid(1000), &mut name);
    in_zone(timestamp_ms, zone)
}

/// Reads `timestamp_ms` in `requested`, or in UTC when that zone's offset is
/// a day or more or the shifted moment would leave 1970 through 9999; nothing
/// for a moment outside that range itself (`inZone`, `:70-100`).
fn in_zone(timestamp_ms: i64, requested: Zone<'_>) -> Option<Reading> {
    const SECONDS_PER_DAY: u32 = 86_400;
    if !(0..=MAX_TIMESTAMP_MS).contains(&timestamp_ms) {
        return None;
    }
    let mut zone = requested;
    // At most 2^31 s, 2.1e12 ms, from a moment under 2.6e14: no overflow.
    let mut local_ms = timestamp_ms + i64::from(zone.offset_s) * 1000;
    if zone.offset_s.unsigned_abs() >= SECONDS_PER_DAY
        || !(0..=MAX_TIMESTAMP_MS).contains(&local_ms)
    {
        zone = Zone::UTC;
        local_ms = timestamp_ms;
    }
    let seconds = local_ms.div_euclid(1000);
    let (year, month, day) = civil(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let (zone, zone_len) = write_zone_name(zone);
    Some(Reading {
        year,
        month,
        day,
        hour: of_day / 3600,
        minute: of_day % 3600 / 60,
        second: of_day % 60,
        millisecond: local_ms.rem_euclid(1000),
        zone,
        zone_len,
    })
}

/// `zone`'s abbreviation when it is a short plain token, and otherwise its
/// offset as `+HHMM`, the form tzdata uses for a zone with no name
/// (`writeZoneName`, `:102-116`). `TZ` comes from the environment, so its
/// abbreviation never reaches the terminal unchecked. `zone.offset_s` is under
/// a day ([`in_zone`]); each digit is taken modulo ten all the same.
fn write_zone_name(zone: Zone<'_>) -> ([u8; MAX_ZONE_NAME_LEN], usize) {
    let mut buf = [0; MAX_ZONE_NAME_LEN];
    if is_plain_zone_name(zone.name) {
        buf[..zone.name.len()].copy_from_slice(zone.name);
        return (buf, zone.name.len());
    }
    let minutes = zone.offset_s.unsigned_abs() / 60;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    let digit = |n: u32| b'0' + (n % 10) as u8;
    buf[0] = if zone.offset_s < 0 { b'-' } else { b'+' };
    buf[1] = digit(hours / 10);
    buf[2] = digit(hours);
    buf[3] = digit(minutes / 10);
    buf[4] = digit(minutes);
    (buf, 5)
}

/// 1 to 10 bytes, each a letter, a digit, `+` or `-` (`isPlainZoneName`,
/// `:118-124`).
fn is_plain_zone_name(name: &[u8]) -> bool {
    (1..=MAX_ZONE_NAME_LEN).contains(&name.len())
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-'))
}

#[cfg(test)]
thread_local! {
    /// The zone a test reads local time in, in place of the machine's, so a
    /// rendered stamp is the same wherever tests run (upstream's `test_zone`,
    /// `:24-27`); `None` reads the C library's, as a release build does.
    /// Per thread: tests run side by side.
    static TEST_ZONE: Cell<Option<Zone<'static>>> = const { Cell::new(Some(Zone::UTC)) };
}

/// Runs `f` with this thread's readings in `zone` (`None`: the C library's),
/// and puts the zone it found back after, however `f` ends.
#[cfg(test)]
pub(crate) fn with_test_zone<R>(zone: Option<Zone<'static>>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Zone<'static>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_ZONE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(TEST_ZONE.with(|slot| slot.replace(zone)));
    f()
}

/// The zone in effect at `epoch_s`, its name copied into `name`.
fn zone_at(epoch_s: i64, name: &mut [u8; MAX_ZONE_NAME_LEN + 1]) -> Zone<'_> {
    #[cfg(test)]
    if let Some(zone) = TEST_ZONE.with(Cell::get) {
        return zone;
    }
    libc_zone_at(epoch_s, name)
}

/// Every reading of the C library's zone, from the call to the copy of its
/// name; `true` once `tzset` has run.
#[cfg(any(target_os = "linux", target_os = "macos"))]
static LIBC_ZONE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

#[cfg(any(target_os = "linux", target_os = "macos"))]
extern "C" {
    /// POSIX `tzset(3)`: the `libc` crate binds it only for Windows.
    fn tzset();
}

/// The local zone in effect at `epoch_s`, from `localtime_r`, or UTC when the
/// C library cannot convert the moment or gives an offset that is not a
/// 32-bit number of seconds (`localZoneAt`, `:144-159`). The name is copied
/// into `name`, at most its length: one byte more than a reading shows, so a
/// longer name is still too long.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn libc_zone_at(epoch_s: i64, name: &mut [u8; MAX_ZONE_NAME_LEN + 1]) -> Zone<'_> {
    // Every moment `local` passes, 0 through 253_402_300_799 s, is a 64-bit
    // `time_t` on both targets.
    let timer = epoch_s as libc::time_t;
    let mut ready = LIBC_ZONE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*ready {
        // SAFETY: `tzset` takes nothing and reads `TZ`, which xfx never sets
        // while it runs; under the lock no reading here is in progress.
        unsafe { tzset() };
        *ready = true;
    }
    // SAFETY: a `tm` of zeros is a valid value -- integers and one null
    // pointer -- for `localtime_r` to overwrite.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to locals that outlive the call.
    if unsafe { libc::localtime_r(&timer, &mut tm) }.is_null() {
        return Zone::UTC;
    }
    let Ok(offset_s) = i32::try_from(tm.tm_gmtoff) else {
        return Zone::UTC;
    };
    let zone = tm.tm_zone.cast::<u8>();
    let mut len = 0;
    if !zone.is_null() {
        while len < name.len() {
            // SAFETY: `tm_zone` is the C library's NUL-terminated string, read
            // up to its NUL and no further, under the lock that keeps another
            // reading here from overwriting it.
            let byte = unsafe { *zone.add(len) };
            if byte == 0 {
                break;
            }
            name[len] = byte;
            len += 1;
        }
    }
    drop(ready);
    Zone {
        offset_s,
        name: &name[..len],
    }
}

/// UTC: a target with no `tm_gmtoff`/`tm_zone` this module reads (upstream
/// reads UTC on WASI, `:150`).
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn libc_zone_at(_epoch_s: i64, _name: &mut [u8; MAX_ZONE_NAME_LEN + 1]) -> Zone<'_> {
    Zone::UTC
}

#[cfg(test)]
mod tests;
