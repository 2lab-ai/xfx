#!/usr/bin/env python3
"""Train 11, round 3: the young-boot-clock harness, for a `git clone
--no-local` only (never the worktree). Patches the clone's
`src/provider/catalog_clock.rs` so that, in a test build, the system catalog
clock reads as a machine's that booted `XFX_YOUNG_BOOT_SECS` seconds (default
150) before this process's first reading of it -- the GitHub arm64 runner
PR #89's gate ran on, whose boot clock had counted more than a minute and
less than five when the lib tests ran (ci-young-clock.txt). The machine's
own clock still moves underneath; only its origin moves.

    python3 -I -B young_clock.py <clone>              # the harness
    python3 -I -B young_clock.py --sub-test-only <clone>
        # instead: `CatalogTime - Duration` (the saturating `Sub`) made
        # `#[cfg(test)]`, so a non-test build that used it would not compile

Each replacement must match once, or nothing is written.
"""
import sys

sub_only = sys.argv[1] == "--sub-test-only"
clone = sys.argv[-1]
path = f"{clone}/src/provider/catalog_clock.rs"
text = open(path, encoding="utf-8").read()
if sub_only:
    old = "impl std::ops::Sub<Duration> for CatalogTime {"
    new = "#[cfg(test)]\nimpl std::ops::Sub<Duration> for CatalogTime {"
else:
    old = "            Source::System => system::boot(),\n"
    new = ("            Source::System => system::boot().map(young_boot),\n")
assert text.count(old) == 1, (old, text.count(old))
text = text.replace(old, new)
if not sub_only:
    text += '''
/// The harness (receipt train-11, young_clock.py): the system catalog clock
/// read as a machine's booted `XFX_YOUNG_BOOT_SECS` (150) seconds before this
/// process's first reading.
fn young_boot(at: Duration) -> Duration {
    static OFFSET: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    let young = std::env::var("XFX_YOUNG_BOOT_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(150));
    let offset = *OFFSET.get_or_init(|| at.saturating_sub(young));
    at.saturating_sub(offset)
}
'''
open(path, "w", encoding="utf-8").write(text)
print(f"{path.split(clone + '/', 1)[1]}: {'Sub made #[cfg(test)]' if sub_only else 'the young boot clock put in'}")
