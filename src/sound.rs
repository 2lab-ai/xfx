//! `/sound`: upstream's switch for its notification sounds, which on xfx is the
//! terminal bell.
//!
//! Upstream's `/sound [on|off|max]` (`vercel-labs/fx@c1db9191
//! src/core/app/app_commands.zig:3959-4029`) moves two switches together --
//! `turn_end` and `attention_required` -- saves both, and says
//! `* sound: <level>`. What it then plays is a set of AAC chimes embedded in
//! its binary, through `/usr/bin/afplay` on macOS, and the terminal bell
//! everywhere else and whenever a chime cannot be played
//! (`src/core/notifications/sound.zig:152-215`). xfx ships no chimes and spawns
//! no player, so what it has is the bell: the same two events, the same two
//! switches, the same file, and one byte.
//!
//! **`max` is not offered.** Upstream's `max` adds UI cues on top of `on` -- a
//! chime when the input is cleared and when the slash menu opens
//! (`src/main.zig:807-813`) -- and those are chimes, not bells. A `max` that
//! set a flag nothing reads would be a level that does nothing, so the grammar
//! is `[on|off]` and `max` is answered with the usage line.
//!
//! This module is what both front ends share: the grammar, the words, and the
//! write. Ringing is the TUI's (`crate::tui`'s `sound` module), because the
//! bell is a byte on a terminal only the TUI writes through a checked frame
//! writer.

use std::io;
use std::path::Path;

use serde_json::Value;

use crate::config::{Notifications, NOTIFICATIONS_KEY};

/// The notice topic upstream says its answer under.
pub const TOPIC: &str = "sound";

/// The refusal for an argument that is not a level, upstream's
/// (`app_commands.zig:3984-3988`) less the `max` xfx does not have.
pub const USAGE: &str = "usage: /sound [on|off]";

/// The two keys under [`NOTIFICATIONS_KEY`] a level is saved as, in the order
/// they are written.
const SWITCHES: [&str; 2] = ["turn_end", "attention_required"];

/// The most bytes of a write failure quoted into a notice.
const MAX_REASON_BYTES: usize = 200;

/// Where the switches stand, as `/sound` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Off,
    On,
}

impl Level {
    /// The word the notice says (`SoundLevel.label`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    /// The two switches this level sets: both on or both off, which is the
    /// only way `/sound` ever sets them (`setNotificationPreferences`,
    /// `src/main.zig:759-770`).
    pub fn notifications(self) -> Notifications {
        Notifications::both(self == Self::On)
    }
}

/// What one `/sound` line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// A bare `/sound`.
    Toggle,
    /// `/sound on` or `/sound off`.
    Set(Level),
}

impl Request {
    /// The command's argument, read upstream's way (`parseSoundCommand`,
    /// `app_commands.zig:3959-3972`): split on spaces and tabs, nothing is a
    /// toggle, one exact lower-case level is that level, and anything else --
    /// another word, a second word, `ON`, `max` -- is `None`, the usage line.
    pub fn parse(argument: &str) -> Option<Self> {
        let mut words = argument.split([' ', '\t']).filter(|word| !word.is_empty());
        let Some(first) = words.next() else {
            return Some(Self::Toggle);
        };
        let level = match first {
            "off" => Level::Off,
            "on" => Level::On,
            _ => return None,
        };
        words.next().is_none().then_some(Self::Set(level))
    }

    /// The level this request moves `current` to.
    ///
    /// A toggle reads `turn_end` alone and never lands anywhere but `on` or
    /// `off`, as upstream's does ("max is only ever explicit",
    /// `app_commands.zig:3977-3978`).
    pub fn level(self, current: Notifications) -> Level {
        match self {
            Self::Toggle if current.turn_end => Level::Off,
            Self::Toggle => Level::On,
            Self::Set(level) => level,
        }
    }
}

/// Saves `level` into the settings file at `path`: both switches, each through
/// the shared preference writer.
///
/// Two writes rather than one: the writer sets one key, and keeping it to one
/// key is what keeps every other key the file holds -- including upstream's
/// `max`, should a hand-edited profile carry one. `turn_end` goes first because
/// it is the one a toggle reads; a failure between the two leaves a file whose
/// next toggle still lands where this one meant to, and is reported all the
/// same.
pub fn save(path: &Path, level: Level) -> io::Result<()> {
    let on = level == Level::On;
    for switch in SWITCHES {
        crate::provider::profile::write_preference(
            path,
            &[NOTIFICATIONS_KEY, switch],
            Value::Bool(on),
        )?;
    }
    Ok(())
}

/// What a save that failed is reported as (`reportUserSettingsFailure`,
/// `src/core/session/session_commands.zig:110-133`).
///
/// `changed` is whether the switches in force moved: upstream says the setting
/// is "active for this process" only when this process really did change. The
/// reason is the error's own words made one inert line, because it can quote
/// a path and a parser's message about a file someone else wrote.
pub fn not_saved(changed: bool, reason: &str) -> String {
    format!(
        "{}not saved to user settings ({})",
        if changed {
            "active for this process but "
        } else {
            ""
        },
        crate::output::safe_one_line(reason, MAX_REASON_BYTES)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argument_is_a_toggle_one_exact_level_or_the_usage_line() {
        for (argument, request) in [
            ("", Some(Request::Toggle)),
            ("on", Some(Request::Set(Level::On))),
            ("off", Some(Request::Set(Level::Off))),
            (" \ton \t", Some(Request::Set(Level::On))),
            // Upstream's third level is not this build's.
            ("max", None),
            // Exact and case-sensitive, as upstream's `std.mem.eql`.
            ("ON", None),
            ("Off", None),
            ("bogus", None),
            // One word only.
            ("on off", None),
            ("on\tnow", None),
        ] {
            assert_eq!(Request::parse(argument), request, "{argument:?}");
        }
    }

    #[test]
    fn a_toggle_reads_turn_end_and_lands_on_on_or_off() {
        let on = Notifications::both(true);
        let off = Notifications::both(false);
        assert_eq!(Request::Toggle.level(on), Level::Off);
        assert_eq!(Request::Toggle.level(off), Level::On);
        // `turn_end` alone decides, as upstream's `if (current.turn_end)`.
        let split = Notifications {
            turn_end: false,
            attention_required: true,
        };
        assert_eq!(Request::Toggle.level(split), Level::On);
        // A set is the level it names, whatever stood before.
        assert_eq!(Request::Set(Level::On).level(on), Level::On);
        assert_eq!(Request::Set(Level::Off).level(off), Level::Off);
    }

    #[test]
    fn a_level_is_both_switches_and_says_its_name() {
        assert_eq!(Level::On.notifications(), Notifications::both(true));
        assert_eq!(Level::Off.notifications(), Notifications::both(false));
        assert_eq!(Level::On.label(), "on");
        assert_eq!(Level::Off.label(), "off");
    }

    #[test]
    fn a_save_writes_both_switches_and_keeps_the_rest_of_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"model":"x/y","notifications":{"max":true}}"#).expect("seed");
        save(&path, Level::On).expect("save on");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "model": "x/y",
                "notifications": {"max": true, "turn_end": true, "attention_required": true}
            })
        );
        save(&path, Level::Off).expect("save off");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(written["notifications"]["turn_end"], false);
        assert_eq!(written["notifications"]["attention_required"], false);
    }

    #[test]
    fn a_failed_save_says_whether_this_process_changed_anyway() {
        assert_eq!(
            not_saved(true, "Permission denied (os error 13)"),
            "active for this process but not saved to user settings (Permission denied (os error 13))"
        );
        assert_eq!(
            not_saved(false, "it is not valid JSON"),
            "not saved to user settings (it is not valid JSON)"
        );
        // The reason is one inert line.
        assert!(!not_saved(true, "a\nb\u{1b}[2J").contains(['\n', '\u{1b}']));
    }
}
