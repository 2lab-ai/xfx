//! The usage cache: one file per sign-in under `~/.xfx/usage-cache/`, fx
//! v0.0.16's `~/.fx/usage-cache/` (`vercel-labs/fx@481104fd
//! src/usage/cache.zig`).
//!
//! `/usage` draws what is cached at once and fills it from AI Gateway in the
//! background (U3). Every xfx process reads the file without a lock -- a write
//! lands by rename, so a reader sees the old file or the new one -- and writes
//! it under an exclusive lock as read, merge, write, rename, so a newer answer
//! is never replaced by an older one (fx's `CacheNeverRegresses`,
//! `cache.zig:1-7`).
//!
//! # The file
//!
//! `<home>/.xfx/usage-cache/<identity>.json`, beside `<identity>.lock` and,
//! for the moment between write and rename, `<identity>.tmp`. Its schema is
//! fx's, version 2 (`Wire`, `cache.zig:172-195`): `schema_version`,
//! `identity`, `today {day, at_ms, models}`, `finished {day, at_ms, days_7,
//! month}`, `budget {at_ms, value}`, and fx's `codex` and `grok` plan parts,
//! which xfx has no sign-in for: written `null`, and a file that holds one is
//! not xfx's to read.
//!
//! # What is trusted to it
//!
//! - **Never the key.** The file is named by, and holds, a SHA-256 over the
//!   sign-in kind, its subject (for an API key, the key itself) and the team
//!   (`identityOf`, `cache.zig:21-35`), the formula and the prefix fx's own,
//!   so a fixture seeded for fx's cache seeds this one. [`Identity`] is the
//!   only thing this module is given; the key stays in
//!   [`super::report::Question`]. That module also refuses a report whose
//!   model name spells the key (`report::KeySpellings`): any 12-byte run of
//!   it as is, in hex of either case, or in base64, standard or URL-safe, at
//!   each of the three byte alignments -- where a shifted run keeps only its
//!   whole base64 groups, so the shortest run looked for in base64 is 9 bytes.
//!   Other encodings (percent-escapes, base32, a reversal) are not looked for.
//!   **That refusal guards against an accidental echo, not a hostile
//!   Gateway**: the configured Gateway is trusted with the key -- it is where
//!   the bearer rule sends it -- and the session log keeps its reply text
//!   anyway (`SessionEvent::AssistantMessage`).
//!   **The digest assumes a high-entropy key**: it is unsalted and its prefix
//!   public, so a short key could be guessed offline from a file name.
//! - **Server text as text.** A model name is the Gateway's string
//!   ([`ModelRow::model`]): written as an escaped JSON string, read back as
//!   the same string, and never part of a path -- every name below the
//!   profile is the identity's hex.
//! - **Owner-only, never through a link.** `~/.xfx` and `usage-cache` are
//!   opened without following a link and must be directories this user owns
//!   with nothing granted to group or other -- the session store's rule
//!   (`session::store`'s `verify_store_dir`) -- or the cache is refused, on
//!   read and on write, rather than repaired. One this module makes is `0700`
//!   and every file `0600`, set on the descriptor after it is made, so the
//!   umask cannot widen or narrow them. Every file is opened without following
//!   a link, and a link, a hard-linked file, or anything that is not a regular
//!   file where one belongs is refused and left as it is.
//! - **A trusted directory, cooperating writers.** The lock is advisory: it
//!   orders xfx processes, and nothing stops another process of this user
//!   writing the directory -- it could, for one, put something else in the
//!   stage's place between its write and its rename. Its deadline
//!   ([`store`]) is the wait for another holder, not a bound on a whole
//!   store.
//! - **Bounded, whole or not at all.** A file over [`MAX_FILE_BYTES`], from
//!   another schema or identity, or with one number or name fx would refuse
//!   reads as absent (`decode`, `cache.zig:197-236`); it is never partly used.
//! - **No home, no cache.** An unset, empty or relative `HOME` gives no
//!   [`Home`] (fx's `openHome`, `usage.zig:524-529`, from 69a01a40), so
//!   nothing is ever written relative to the current directory.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::report::{utc_day, ModelRow, Totals, MAX_DAY, MAX_MODELS, MAX_MODEL_NAME_BYTES};

/// The cache's directory under the profile (`dir_name`, `cache.zig:17`).
pub const DIR_NAME: &str = "usage-cache";

/// The schema this module writes and the only one it reads
/// (`schema_version`, `cache.zig:18`).
pub const SCHEMA_VERSION: u32 = 2;

/// The largest cache file read (`max_file_bytes`, `cache.zig:19`).
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// fx's name for the API key sign-in kind, as the identity hashes it
/// (`@tagName(Scope.api_key)`, `report.zig:20`).
pub(crate) const API_KEY_SCOPE: &str = "api_key";

/// What every identity's hash starts with (`cache.zig:28`).
const IDENTITY_PREFIX: &[u8] = b"fx-usage-cache-v1\x00";

/// How long a store waits for another process's lock before giving up.
#[cfg(unix)]
const LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often a waiting store tries the lock again.
#[cfg(unix)]
const LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(10);

/// Whose numbers an entry holds: SHA-256, in lowercase hex, of the sign-in
/// kind, a stable subject and the team (`Identity`, `identityOf`,
/// `cache.zig:21-35`). The subject never reaches the file, and a digest says
/// nothing of it that could be read back.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity([u8; 64]);

impl Identity {
    /// The identity of `subject` signed in as `scope`, on `team` (empty for
    /// none).
    pub(crate) fn of(scope: &str, subject: &str, team: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(IDENTITY_PREFIX);
        hash.update(scope.as_bytes());
        hash.update(b"\x00");
        hash.update(subject.as_bytes());
        hash.update(b"\x00");
        hash.update(team.as_bytes());
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut hex = [0u8; 64];
        for (i, byte) in hash.finalize().iter().enumerate() {
            hex[2 * i] = HEX[usize::from(byte >> 4)];
            hex[2 * i + 1] = HEX[usize::from(byte & 0x0f)];
        }
        Self(hex)
    }

    /// The hex digest, which names the files and is written in them.
    pub fn as_str(&self) -> &str {
        // Only ever built from the ASCII digits above.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// The file name for this identity with `suffix`.
    #[cfg(unix)]
    fn file(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.as_str())
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({})", self.as_str())
    }
}

/// Today's per-model numbers, from a fetch that started at `at_ms` (`Today`,
/// `cache.zig:37-42`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Today {
    pub day: i64,
    pub at_ms: i64,
    #[serde(deserialize_with = "bounded_rows")]
    pub models: Vec<ModelRow>,
}

/// The finished days before `day`: the 7d window's and this calendar
/// month's, empty on the 1st (`Finished`, `cache.zig:44-51`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finished {
    pub day: i64,
    pub at_ms: i64,
    #[serde(deserialize_with = "bounded_rows")]
    pub days_7: Vec<ModelRow>,
    #[serde(deserialize_with = "bounded_rows")]
    pub month: Vec<ModelRow>,
}

/// A budget, from a fetch that started at `at_ms` (`Budget`,
/// `cache.zig:53-56`). Only an fx login's team has one, so xfx keeps what a
/// file holds and fetches none yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub at_ms: i64,
    pub value: super::report::Budget,
}

/// One sign-in's cached numbers (`Entry`, `cache.zig:70-76`, less the plan
/// parts xfx cannot fetch).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub today: Option<Today>,
    pub finished: Option<Finished>,
    pub budget: Option<Budget>,
}

impl Entry {
    /// Whether [`read`] would keep this entry (`validEntry`,
    /// `cache.zig:216-236`): at most [`MAX_MODELS`] rows a part, each named in
    /// 1 to [`MAX_MODEL_NAME_BYTES`] bytes, every number finite and not
    /// negative. And, xfx's own: every period the dashboard adds up stays
    /// finite, so no total it shows is an overflow. fx's `own` is this check.
    pub fn valid(&self) -> bool {
        let today: &[ModelRow] = self.today.as_ref().map_or(&[], |t| &t.models);
        let mut parts: Vec<&[ModelRow]> = vec![today];
        if let Some(finished) = &self.finished {
            parts.push(&finished.days_7);
            parts.push(&finished.month);
        }
        if !parts.iter().all(|rows| valid_rows(rows)) {
            return false;
        }
        if let Some(budget) = &self.budget {
            if !valid_money(budget.value.limit) || !valid_money(budget.value.spent) {
                return false;
            }
        }
        // Days and stamps a clock could have written, so no caller's day or
        // age arithmetic overflows on one read from a file.
        let days = [
            self.today.as_ref().map(|t| t.day),
            self.finished.as_ref().map(|f| f.day),
        ];
        let stamps = [
            self.today.as_ref().map(|t| t.at_ms),
            self.finished.as_ref().map(|f| f.at_ms),
            self.budget.as_ref().map(|b| b.at_ms),
        ];
        if !days
            .into_iter()
            .flatten()
            .all(|day| (0..=MAX_DAY).contains(&day))
            || !stamps.into_iter().flatten().all(|at_ms| at_ms >= 0)
        {
            return false;
        }
        // Today alone, and each finished window with today (`view.zig:84`).
        parts.iter().enumerate().all(|(i, rows)| {
            let mut sum = Totals::default();
            for row in rows.iter().chain(if i == 0 { &[][..] } else { today }) {
                sum.add(&row.totals);
            }
            sum.valid()
        })
    }
}

fn valid_rows(rows: &[ModelRow]) -> bool {
    rows.len() <= MAX_MODELS
        && rows.iter().all(|row| {
            !row.model.is_empty() && row.model.len() <= MAX_MODEL_NAME_BYTES && row.totals.valid()
        })
}

/// A list of rows, refused at its 257th, before the rest are read: a struct can
/// be read from a JSON array, so a row can take as little as nine bytes, and a
/// 4 MiB file would otherwise become half a million of them before
/// [`Entry::valid`] counted.
fn bounded_rows<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ModelRow>, D::Error> {
    struct Rows;

    impl<'de> de::Visitor<'de> for Rows {
        type Value = Vec<ModelRow>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX_MODELS} model rows")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while rows.len() < MAX_MODELS {
                match seq.next_element()? {
                    Some(row) => rows.push(row),
                    None => return Ok(rows),
                }
            }
            match seq.next_element::<de::IgnoredAny>()? {
                None => Ok(rows),
                Some(_) => Err(de::Error::custom("more than 256 models")),
            }
        }
    }

    deserializer.deserialize_seq(Rows)
}

fn valid_money(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

/// Whether a fetch at `now_ms` must read the finished days: the cache lacks
/// today's copy of them, so they cost at most one read per UTC day (the
/// `finished` half of `plan`, `policy.zig:55-89`).
pub fn finished_due(entry: Option<&Entry>, now_ms: i64) -> bool {
    entry
        .and_then(|e| e.finished.as_ref())
        .is_none_or(|finished| finished.day != utc_day(now_ms))
}

/// Combines what was stored with what a fetch returned, at wall time `now_ms`
/// (`merge`, `cache.zig:88-113`).
///
/// Each part keeps the newer of the two: today by fetch start, finished days
/// by day, the budget by fetch start; on a tie the fetch wins. A stored part
/// stamped after `now_ms` was written while the clock ran ahead, so it yields
/// to the fetch; otherwise a backward clock correction would keep every later
/// fetch out until the clock caught up. Finished days do not change during a
/// day, so they count as ahead only when they are for a later day, and that
/// holds for the fetch too: one that started before the clock was corrected
/// back across midnight read them for a day that has not begun.
pub fn merge(stored: Entry, fetched: Entry, now_ms: i64) -> Entry {
    let day = utc_day(now_ms);
    Entry {
        today: newer(
            stored.today.filter(|t| t.at_ms <= now_ms),
            fetched.today,
            |t| t.at_ms,
        ),
        finished: newer(
            stored.finished.filter(|f| f.day <= day),
            fetched.finished.filter(|f| f.day <= day),
            |f| f.day,
        ),
        budget: newer(
            stored.budget.filter(|b| b.at_ms <= now_ms),
            fetched.budget,
            |b| b.at_ms,
        ),
    }
}

/// The newer of two parts by `key`, the fetched one on a tie (`newer`,
/// `cache.zig:166-170`).
fn newer<T>(stored: Option<T>, fetched: Option<T>, key: impl Fn(&T) -> i64) -> Option<T> {
    match (stored, fetched) {
        (None, fetched) => fetched,
        (stored, None) => stored,
        (Some(stored), Some(fetched)) => Some(if key(&fetched) >= key(&stored) {
            fetched
        } else {
            stored
        }),
    }
}

/// The file as written: fx's `Wire` (`cache.zig:172-195`), its fields in
/// fx's order and every absent part `null`.
#[derive(Serialize)]
struct WireOut<'a> {
    schema_version: u32,
    identity: &'a str,
    today: Option<&'a Today>,
    finished: Option<&'a Finished>,
    budget: Option<&'a Budget>,
    codex: Option<()>,
    grok: Option<()>,
}

/// The file as read. A field fx's `Wire` does not have refuses it, as fx's
/// parse does (`parseFromSliceLeaky` without `ignore_unknown_fields`), and so
/// does a key twice in one object.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireIn {
    schema_version: u32,
    identity: String,
    #[serde(default)]
    today: Option<Today>,
    #[serde(default)]
    finished: Option<Finished>,
    #[serde(default)]
    budget: Option<Budget>,
    #[serde(default, rename = "codex")]
    _codex: Option<NoPlan>,
    #[serde(default, rename = "grok")]
    _grok: Option<NoPlan>,
}

/// A Codex or Grok plan part: `null` is read as none, and anything else
/// refuses the file, because xfx has no reader for either.
struct NoPlan;

impl<'de> Deserialize<'de> for NoPlan {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(de::Error::custom("a plan part xfx does not read"))
    }
}

/// The file's bytes for `entry` (`encode`, `cache.zig:182-195`); `None` only
/// when serde cannot write it.
fn encode(identity: &Identity, entry: &Entry) -> Option<Vec<u8>> {
    serde_json::to_vec(&WireOut {
        schema_version: SCHEMA_VERSION,
        identity: identity.as_str(),
        today: entry.today.as_ref(),
        finished: entry.finished.as_ref(),
        budget: entry.budget.as_ref(),
        codex: None,
        grok: None,
    })
    .ok()
}

/// Parses a cache file for `identity` (`decode`, `cache.zig:197-214`).
/// Anything malformed, oversized, from another schema, for another identity,
/// or holding a number or name [`Entry::valid`] refuses reads as absent.
fn decode(identity: &Identity, bytes: &[u8]) -> Option<Entry> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let wire: WireIn = serde_json::from_slice(bytes).ok()?;
    if wire.schema_version != SCHEMA_VERSION || wire.identity != identity.as_str() {
        return None;
    }
    let entry = Entry {
        today: wire.today,
        finished: wire.finished,
        budget: wire.budget,
    };
    entry.valid().then_some(entry)
}

/// The home directory the cache lives under: `<home>/.xfx/usage-cache`.
///
/// Exists only for an absolute path. An unset, empty or relative `HOME` is no
/// home at all, as everywhere else in fx (`openHome`, `usage.zig:524-529`),
/// so a cache can never land in whatever directory xfx was started from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    path: PathBuf,
}

impl Home {
    /// The cache's home for `home`, as `RuntimeConfig::home` gives it, or
    /// `None` when there is no absolute one.
    pub fn new(home: Option<&Path>) -> Option<Self> {
        let path = home?;
        path.is_absolute().then(|| Self {
            path: path.to_path_buf(),
        })
    }
}

/// Why [`store`] wrote nothing.
#[derive(Debug)]
pub enum StoreError {
    /// What would be written is an entry [`read`] would refuse, so the file is
    /// left as it was (fx's `error.InvalidCache`, `cache.zig:287`, which
    /// `/usage` reports as the `cache` failure, `usage.zig:705-708`).
    Invalid,
    /// A link, a hard-linked file, or something that is not a regular file or
    /// directory is where the cache's directory or one of its files belongs;
    /// or `~/.xfx` or `usage-cache` is not private -- another user's, or open
    /// to group or other. It is neither followed, nor replaced, nor repaired.
    Refused(&'static str),
    /// The directory or a file could not be made, locked, written or renamed,
    /// or another process held the lock past the deadline.
    Io(io::Error),
}

/// Reads the stored entry for `identity`, or `None` (`read`,
/// `cache.zig:242-250`). Takes no lock.
pub fn read(home: &Home, identity: &Identity) -> Option<Entry> {
    #[cfg(unix)]
    {
        let dir = fs::open_cache_dir(home).ok()?;
        match fs::load(&dir, identity) {
            fs::Load::Found(entry) => Some(entry),
            fs::Load::Missing | fs::Load::Unusable | fs::Load::Refused(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (home, identity);
        None
    }
}

/// Merges `fetched` into the stored entry under the identity's lock, at wall
/// time `now_ms`, and writes the result by rename (`store`,
/// `cache.zig:261-297`). Returns the entry as [`read`] would now return it.
///
/// `held` is the caller's own copy of the entry: it counts as well, so a
/// process never stores less than it holds, even after the file was
/// unreadable and a store started over from less. Never writes what `read`
/// would refuse, so a bad fetch leaves the file as it was. Creates a missing
/// `~/.xfx` owner-only (`openOrCreateFxDir`, `usage.zig:537-547`).
pub fn store(
    home: &Home,
    identity: &Identity,
    fetched: Entry,
    now_ms: i64,
    held: Option<&Entry>,
) -> Result<Entry, StoreError> {
    store_now(home, identity, fetched, &|| now_ms, held)
}

/// [`store`], reading the wall time from `clock` once the lock is held rather
/// than taking it as a number.
///
/// A store waits for the lock (up to its deadline), and another process may
/// store a later fetch meanwhile. A time read before the wait would make that
/// later part look stamped ahead of the clock, and [`merge`] would let the
/// older fetch replace it. fx reads its time before the store
/// (`usage.zig:697-705`); xfx reads it under the lock.
pub fn store_now(
    home: &Home,
    identity: &Identity,
    fetched: Entry,
    clock: &dyn Fn() -> i64,
    held: Option<&Entry>,
) -> Result<Entry, StoreError> {
    #[cfg(unix)]
    {
        fs::store_within(home, identity, fetched, clock, held, LOCK_DEADLINE)
    }
    #[cfg(not(unix))]
    {
        let _ = (home, identity, fetched, clock, held);
        Err(StoreError::Io(io::ErrorKind::Unsupported.into()))
    }
}

/// Test-only: runs `hook` each time a store on the calling thread finds the
/// lock taken, before it waits to try again -- so a test can act once a
/// store is known to be waiting.
#[cfg(all(test, unix))]
pub(crate) fn on_lock_busy(hook: impl FnMut() + 'static) {
    fs::seam::on_lock_busy(hook);
}

/// The base a store merges the fetch into: what the file holds and what the
/// caller holds, the file winning a tie (`cache.zig:285`).
fn base_of(stored: Option<Entry>, held: Option<&Entry>, now_ms: i64) -> Entry {
    match (stored, held) {
        (Some(stored), Some(held)) => merge(held.clone(), stored, now_ms),
        (Some(stored), None) => stored,
        (None, held) => held.cloned().unwrap_or_default(),
    }
}

/// The cache's file system half: every name opened relative to a descriptor,
/// and from `~/.xfx` down never through a link.
#[cfg(unix)]
mod fs {
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::fd::{AsFd, OwnedFd};

    use rustix::fs::{AtFlags, FileType, FlockOperation, Mode, OFlags, CWD};
    use rustix::io::Errno;

    use super::{
        base_of, decode, encode, merge, Entry, Home, Identity, StoreError, DIR_NAME, LOCK_RETRY,
        MAX_FILE_BYTES,
    };

    /// The profile directory's name under the home (`config::PROFILE_DIR_NAME`).
    const PROFILE: &str = crate::config::PROFILE_DIR_NAME;

    /// How many times a store retries creating the lock it lost a race for.
    const CREATE_RACES: u32 = 50;

    /// `0700` and `0600`.
    const DIR_MODE: Mode = Mode::RWXU;
    const FILE_MODE: Mode = Mode::RUSR.union(Mode::WUSR);

    /// Test-only: a store in a copy of the test binary given this variable
    /// kills itself between writing the stage and renaming it.
    #[cfg(test)]
    pub(super) const KILL_BEFORE_RENAME: &str = "XFX_TEST_USAGE_CACHE_KILL_BEFORE_RENAME";

    fn io_err(err: Errno) -> StoreError {
        StoreError::Io(err.into())
    }

    /// What is where an entry's file belongs.
    pub(super) enum Load {
        /// No file.
        Missing,
        /// A regular file that is not an entry for this identity: too large,
        /// unreadable, malformed. A store replaces it, as fx's does.
        Unusable,
        /// Something that must not be read or replaced: a link, a hard-linked
        /// file, a directory, a FIFO, a device.
        Refused(&'static str),
        Found(Entry),
    }

    fn open_home(home: &Home) -> Result<OwnedFd, Errno> {
        rustix::fs::openat(
            CWD,
            &home.path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
    }

    /// A directory under `parent`, never through a link: `ELOOP` on Linux and
    /// `ENOTDIR` on macOS for one, `ENOTDIR` for a file.
    fn open_dir_in(
        parent: &OwnedFd,
        name: &str,
        refusal: &'static str,
    ) -> Result<OwnedFd, StoreError> {
        rustix::fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|err| match err {
            Errno::LOOP | Errno::NOTDIR => StoreError::Refused(refusal),
            other => io_err(other),
        })
    }

    /// `~/.xfx`, as the session store takes it: never through a link.
    fn open_profile(home: &OwnedFd) -> Result<OwnedFd, StoreError> {
        open_dir_in(home, PROFILE, "~/.xfx is a link or not a directory")
    }

    /// `usage-cache` itself.
    fn open_dir_at(profile: &OwnedFd) -> Result<OwnedFd, StoreError> {
        open_dir_in(
            profile,
            DIR_NAME,
            "usage-cache is a link or not a directory",
        )
    }

    /// What is wrong with a directory of `file_type`, owned by `uid` with
    /// `mode`, for this user `euid` to keep the cache in: the session store's
    /// rule (`verify_store_dir`), a directory this user owns with nothing
    /// granted to group or other.
    pub(super) fn private_dir_problem(
        file_type: FileType,
        uid: u32,
        mode: Mode,
        euid: u32,
    ) -> Option<&'static str> {
        if file_type != FileType::Directory {
            Some("not a directory")
        } else if uid != euid {
            Some("a directory another user owns")
        } else if !(mode & (Mode::RWXG | Mode::RWXO)).is_empty() {
            Some("a directory open to group or other")
        } else {
            None
        }
    }

    /// Refuses the directory `fd` unless it is private to this user.
    pub(super) fn verify_private_dir(fd: &OwnedFd) -> Result<(), StoreError> {
        let stat = rustix::fs::fstat(fd).map_err(io_err)?;
        match private_dir_problem(
            FileType::from_raw_mode(stat.st_mode),
            stat.st_uid,
            Mode::from_raw_mode(stat.st_mode),
            rustix::process::geteuid().as_raw(),
        ) {
            Some(problem) => Err(StoreError::Refused(problem)),
            None => Ok(()),
        }
    }

    /// The cache directory for reading, if there is one this user keeps
    /// privately.
    pub(super) fn open_cache_dir(home: &Home) -> Result<OwnedFd, StoreError> {
        let home = open_home(home).map_err(io_err)?;
        let profile = open_profile(&home)?;
        verify_private_dir(&profile)?;
        let dir = open_dir_at(&profile)?;
        verify_private_dir(&dir)?;
        Ok(dir)
    }

    /// Sets `fd`'s permission bits to exactly `wanted`, whatever the umask
    /// made them.
    fn private(fd: impl AsFd, wanted: Mode) -> Result<(), StoreError> {
        let stat = rustix::fs::fstat(&fd).map_err(io_err)?;
        let bits = Mode::from_raw_mode(stat.st_mode) & (Mode::RWXU | Mode::RWXG | Mode::RWXO);
        if bits != wanted {
            rustix::fs::fchmod(&fd, wanted).map_err(io_err)?;
        }
        Ok(())
    }

    /// Why the entry `name` in `dir` must not be opened, looked at rather than
    /// guessed from an errno: the platforms disagree on what opening a link,
    /// a socket or a directory reports. `None` for a regular file or nothing.
    fn refused_entry(dir: &OwnedFd, name: &str) -> Option<&'static str> {
        let stat = rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
        match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => None,
            FileType::Symlink => Some("a symbolic link"),
            FileType::Directory => Some("a directory"),
            _ => Some("not a regular file"),
        }
    }

    /// Refuses `fd` unless it is a regular file with no other name.
    fn sole_regular(fd: impl AsFd) -> Result<rustix::fs::Stat, StoreError> {
        let stat = rustix::fs::fstat(&fd).map_err(io_err)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(StoreError::Refused("not a regular file"));
        }
        // More than one name is a hard link. None at all is a file a store
        // renamed over after this reader opened it: the whole old file, which
        // a reader without the lock may see.
        if stat.st_nlink > 1 {
            return Err(StoreError::Refused(
                "a file with another name (a hard link)",
            ));
        }
        Ok(stat)
    }

    /// Opens a file in the cache directory without following a link and
    /// without waiting on a FIFO.
    fn open_in(dir: &OwnedFd, name: &str, flags: OFlags) -> Result<OwnedFd, Errno> {
        rustix::fs::openat(
            dir,
            name,
            flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            FILE_MODE,
        )
    }

    /// What the cache directory holds for `identity`.
    pub(super) fn load(dir: &OwnedFd, identity: &Identity) -> Load {
        let name = identity.file(".json");
        let fd = match open_in(dir, &name, OFlags::RDONLY) {
            Ok(fd) => fd,
            Err(Errno::NOENT) => return Load::Missing,
            Err(_) => return refused_entry(dir, &name).map_or(Load::Unusable, Load::Refused),
        };
        let stat = match sole_regular(&fd) {
            Ok(stat) => stat,
            Err(StoreError::Refused(why)) => return Load::Refused(why),
            Err(_) => return Load::Unusable,
        };
        if u64::try_from(stat.st_size).map_or(true, |size| size > MAX_FILE_BYTES) {
            return Load::Unusable;
        }
        #[cfg(test)]
        seam::after_stat();
        let Ok(bytes) = read_capped(File::from(fd)) else {
            return Load::Unusable;
        };
        #[cfg(test)]
        seam::read(bytes.len());
        decode(identity, &bytes).map_or(Load::Unusable, Load::Found)
    }

    /// At most one byte past [`MAX_FILE_BYTES`] of `file`: bounded again as it
    /// is read, in case it grew after its size was looked at.
    pub(super) fn read_capped(file: File) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// The profile directory, made owner-only when it is missing, and refused
    /// -- not repaired -- when one that is there is not private.
    fn open_or_create_profile(home: &Home) -> Result<OwnedFd, StoreError> {
        let home = open_home(home).map_err(io_err)?;
        let made = match rustix::fs::mkdirat(&home, PROFILE, DIR_MODE) {
            Ok(()) => true,
            Err(Errno::EXIST) => false,
            Err(err) => return Err(io_err(err)),
        };
        let profile = open_profile(&home)?;
        if made {
            private(&profile, DIR_MODE)?;
        }
        verify_private_dir(&profile)?;
        Ok(profile)
    }

    /// The cache directory, made `0700` if missing; one that is there must
    /// already be private.
    fn open_or_create_dir(home: &Home) -> Result<OwnedFd, StoreError> {
        let profile = open_or_create_profile(home)?;
        let made = match rustix::fs::mkdirat(&profile, DIR_NAME, DIR_MODE) {
            Ok(()) => true,
            Err(Errno::EXIST) => false,
            Err(err) => return Err(io_err(err)),
        };
        let dir = open_dir_at(&profile)?;
        if made {
            private(&dir, DIR_MODE)?;
        }
        verify_private_dir(&dir)?;
        Ok(dir)
    }

    /// `<identity>.lock`, held exclusively, waiting at most `deadline` for
    /// another process (fx's `.lock = .exclusive`, `cache.zig:276-280`).
    /// Released when the file closes, so a store that dies cannot leave it
    /// held.
    fn lock(
        dir: &OwnedFd,
        identity: &Identity,
        deadline: std::time::Duration,
    ) -> Result<OwnedFd, StoreError> {
        let name = identity.file(".lock");
        let mut lost_races = 0;
        let fd = loop {
            match open_in(dir, &name, OFlags::RDWR | OFlags::CREATE) {
                Ok(fd) => break fd,
                // Two processes creating the same name at once: macOS answers
                // the slower `ENOENT` (measured 2026-11-06, about two in five
                // of 2000 simultaneous creates). The name is there now.
                Err(Errno::NOENT) if lost_races < CREATE_RACES => {
                    lost_races += 1;
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(err) => {
                    return Err(
                        refused_entry(dir, &name).map_or_else(|| io_err(err), StoreError::Refused)
                    )
                }
            }
        };
        sole_regular(&fd)?;
        private(&fd, FILE_MODE)?;
        let started = std::time::Instant::now();
        loop {
            match rustix::fs::flock(&fd, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(fd),
                Err(Errno::WOULDBLOCK) if started.elapsed() < deadline => {
                    #[cfg(test)]
                    seam::lock_busy();
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(Errno::WOULDBLOCK) => {
                    return Err(StoreError::Io(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "another process holds the usage cache's lock",
                    )))
                }
                Err(err) => return Err(io_err(err)),
            }
        }
    }

    /// The stage, removed unless it was renamed into place.
    struct Stage<'a> {
        dir: &'a OwnedFd,
        name: String,
        renamed: bool,
    }

    impl Drop for Stage<'_> {
        fn drop(&mut self) {
            if !self.renamed {
                let _ = rustix::fs::unlinkat(self.dir, self.name.as_str(), AtFlags::empty());
            }
        }
    }

    /// Writes `bytes` to `<identity>.tmp`, owner-only, and renames it over
    /// `<identity>.json` (`cache.zig:289-294`). Runs under the lock, so the
    /// fixed stage name is this store's alone; whatever is there -- a stage a
    /// crash left, or a link -- is unlinked, never written through, and the
    /// stage is created new.
    fn replace(dir: &OwnedFd, identity: &Identity, bytes: &[u8]) -> Result<(), StoreError> {
        let name = identity.file(".tmp");
        match rustix::fs::unlinkat(dir, name.as_str(), AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => {}
            // A directory, which `unlinkat` will not remove.
            Err(Errno::ISDIR | Errno::PERM) => {
                return Err(StoreError::Refused("the stage is a directory"))
            }
            Err(err) => return Err(io_err(err)),
        }
        let mut stage = Stage {
            dir,
            name,
            renamed: false,
        };
        let fd = open_in(
            dir,
            &stage.name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        )
        .map_err(io_err)?;
        private(&fd, FILE_MODE)?;
        let mut file = File::from(fd);
        file.write_all(bytes).map_err(StoreError::Io)?;
        file.sync_all().map_err(StoreError::Io)?;
        drop(file);

        #[cfg(test)]
        if std::env::var_os(KILL_BEFORE_RENAME).is_some() {
            let _ = rustix::process::kill_process(
                rustix::process::getpid(),
                rustix::process::Signal::KILL,
            );
            // The signal may land on another thread a moment later; this one
            // must not reach the rename first.
            loop {
                std::thread::park();
            }
        }

        rustix::fs::renameat(
            dir,
            stage.name.as_str(),
            dir,
            identity.file(".json").as_str(),
        )
        .map_err(io_err)?;
        stage.renamed = true;
        // The rename is durable once the directory entry is.
        rustix::fs::fsync(dir).map_err(io_err)
    }

    /// [`super::store_now`], waiting at most `deadline` for the lock and
    /// reading the time once it holds it.
    pub(super) fn store_within(
        home: &Home,
        identity: &Identity,
        fetched: Entry,
        clock: &dyn Fn() -> i64,
        held: Option<&Entry>,
        deadline: std::time::Duration,
    ) -> Result<Entry, StoreError> {
        let dir = open_or_create_dir(home)?;
        let _lock = lock(&dir, identity, deadline)?;
        let now_ms = clock();
        let stored = match load(&dir, identity) {
            Load::Found(entry) => Some(entry),
            Load::Missing | Load::Unusable => None,
            Load::Refused(why) => return Err(StoreError::Refused(why)),
        };
        let merged = merge(base_of(stored, held, now_ms), fetched, now_ms);
        if !merged.valid() {
            return Err(StoreError::Invalid);
        }
        let bytes = encode(identity, &merged).ok_or(StoreError::Invalid)?;
        // What is returned is what a reader of these bytes gets, and bytes a
        // reader would refuse are never written.
        let entry = decode(identity, &bytes).ok_or(StoreError::Invalid)?;
        replace(&dir, identity, &bytes)?;
        Ok(entry)
    }

    /// Test-only: what runs between a load's size check and its read, and how
    /// many bytes the read took, per thread. Compiled only into tests.
    #[cfg(test)]
    pub(super) mod seam {
        use std::cell::{Cell, RefCell};

        thread_local! {
            static AFTER_STAT: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
            static READ: Cell<Option<usize>> = const { Cell::new(None) };
            static LOCK_BUSY: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None);
        }

        /// Runs `hook` each time a store on this thread finds the lock taken,
        /// before it waits to try again.
        pub(in super::super) fn on_lock_busy(hook: impl FnMut() + 'static) {
            LOCK_BUSY.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        }

        pub(super) fn lock_busy() {
            LOCK_BUSY.with(|slot| {
                if let Some(hook) = slot.borrow_mut().as_mut() {
                    hook();
                }
            });
        }

        /// Runs `hook` in this thread's next load, after the size check.
        pub(in super::super) fn on_after_stat(hook: impl FnOnce() + 'static) {
            AFTER_STAT.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        }

        /// How many bytes this thread's last load read, once.
        pub(in super::super) fn last_read() -> Option<usize> {
            READ.with(Cell::take)
        }

        pub(super) fn after_stat() {
            if let Some(hook) = AFTER_STAT.with(|slot| slot.borrow_mut().take()) {
                hook();
            }
        }

        pub(super) fn read(len: usize) {
            READ.with(|read| read.set(Some(len)));
        }
    }
}

#[cfg(test)]
mod tests;
