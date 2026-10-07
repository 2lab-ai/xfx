//! The tool-call ids a request is sent with, as opposed to the ones it records.
//!
//! A tool call's id is the provider's: the Gateway relays whatever the model
//! behind it minted, and some mint ids no other wire takes -- upstream's own
//! test vectors are `functions.read:0` and `functions/read:0`
//! (`vercel-labs/fx@c1db9191 src/gateway/tool_call_ids.zig:139`). Anthropic's
//! `tool_use.id` is `^[a-zA-Z0-9_-]+$`, so a conversation that carries such a
//! call from the Gateway onto llmux would be refused at its next request. This
//! is upstream's answer, ported as it stands (`Projection`,
//! `tool_call_ids.zig:1-100`): an id that is already portable -- one to 64
//! bytes of ASCII letters, digits, `_` and `-` (`portable`, `:102-109`) -- goes
//! as it is, and any other is sent under an alias derived from it, the same
//! alias every time, at the call and at its result alike. **Only the request
//! is projected**: the session log keeps the id the provider minted, so a
//! resume renders the same aliases again rather than inheriting them.
//!
//! Upstream projects on every wire it writes (`vercel_protocol.zig:331`,
//! `responses_protocol.zig:67`), and so does xfx: the Gateway's
//! ([`super::protocol`]) and llmux's ([`crate::llmux::protocol`]).
//!
//! The alias is upstream's, byte for byte: `fx_<40 hex>_<n>`
//! (`tool_call_ids.zig:61`), the hex the first 20 bytes of the SHA-256 of the
//! source id, lowercase, and `n` stepping past an alias already taken -- so a
//! request carries the id fx's would (`functions.read:0` is
//! `fx_c216cab2de2e56d547b4e6340edfc11a16bd7a07_0` on both, in
//! `.prd/tui-phase4/receipts/p1/`). It is a wire literal the model reads, kept
//! as upstream writes it, as the TUI keeps upstream's `id=fx-N` link ids
//! (`crate::tui::hyperlink`).

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use super::protocol::{ContentPart, Message, ProtocolError, Role};

/// The longest id that goes as it is (`max_id_bytes`, `tool_call_ids.zig:4`).
const MAX_ID_BYTES: usize = 64;

/// What an alias starts with (`tool_call_ids.zig:61`).
const ALIAS_PREFIX: &str = "fx_";

/// One request's wire ids.
///
/// Empty for a request whose ids are all portable, which is the identity --
/// and almost every request: nothing is computed for it beyond the check
/// (`Portable requests allocate nothing`, `tool_call_ids.zig:6`).
#[derive(Debug, Default)]
pub(crate) struct Projection {
    ids: HashMap<String, Entry>,
}

#[derive(Debug)]
struct Entry {
    wire: String,
    /// Named by state this wire replays verbatim, which refers to the call by
    /// the id it was minted with, so it cannot be renamed.
    protected: bool,
}

impl Projection {
    /// The projection that sends every id as it is.
    pub(crate) fn identity() -> Self {
        Self::default()
    }

    /// The ids `messages` are sent with on a wire that replays its own
    /// recorded blocks verbatim when `replays_blocks` -- llmux's, whose
    /// [`ContentPart::RawBlocks`] carry each `tool_use` under the id the call
    /// was minted with. Upstream's `provider_replay != null`
    /// (`tool_call_ids.zig:24`, `:42`, `:54`): a call on such a message is
    /// never renamed, and a renaming that would have to reach one is refused.
    /// On the Gateway the blocks are reduced to their text
    /// ([`super::protocol`]'s `WireParts`), so nothing in them names a call.
    ///
    /// Upstream's `Projection.init`, step for step (`tool_call_ids.zig:14-87`).
    /// Its exhaustion error is not ported: `attempts` there is one more than
    /// the ids known, each attempt is a distinct candidate and at most that
    /// many can be taken, so the loop below always finds one.
    pub(crate) fn of(messages: &[Message], replays_blocks: bool) -> Result<Self, ProtocolError> {
        let replayed = |message: &Message| replays_blocks && message.raw_blocks().is_some();
        let mut needed = false;
        let mut opaque_history = false;
        for message in messages {
            for id in result_ids(message) {
                if id.is_empty() {
                    return Err(ProtocolError::InvalidToolCallId {
                        call_id: id.to_string(),
                    });
                }
                needed = needed || !portable(id);
            }
            if message.role != Role::Assistant {
                continue;
            }
            opaque_history = opaque_history || replayed(message);
            for id in call_ids(message) {
                if id.is_empty() {
                    return Err(ProtocolError::InvalidToolCallId {
                        call_id: id.to_string(),
                    });
                }
                needed = needed || !portable(id);
            }
        }
        if !needed {
            return Ok(Self::identity());
        }

        let mut ids: HashMap<String, Entry> = HashMap::new();
        for message in messages {
            if message.role == Role::Assistant {
                let protected = replayed(message);
                for id in call_ids(message) {
                    let entry = ids.entry(id.to_string()).or_insert_with(|| Entry {
                        wire: id.to_string(),
                        protected: false,
                    });
                    entry.protected = entry.protected || protected;
                }
            }
            for id in result_ids(message) {
                ids.entry(id.to_string()).or_insert_with(|| Entry {
                    wire: id.to_string(),
                    protected: false,
                });
            }
        }
        for message in messages {
            if message.role != Role::Assistant {
                continue;
            }
            for id in call_ids(message) {
                if portable(&ids[id].wire) || replayed(message) {
                    continue;
                }
                if ids[id].protected {
                    return Err(ProtocolError::ProtectedToolCallId {
                        call_id: id.to_string(),
                    });
                }
                let digest = Sha256::digest(id.as_bytes());
                let hex: String = digest[..20]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                let mut attempt = 0usize;
                let alias = loop {
                    let candidate = format!("{ALIAS_PREFIX}{hex}_{attempt}");
                    if !ids.contains_key(&candidate) {
                        break candidate;
                    }
                    // State replayed verbatim may name an alias an earlier
                    // request sent, so stepping past one could rename a call
                    // that state refers to (`tool_call_ids.zig:63-66`).
                    if opaque_history {
                        return Err(ProtocolError::ProtectedToolCallId {
                            call_id: id.to_string(),
                        });
                    }
                    attempt += 1;
                };
                ids.insert(
                    alias.clone(),
                    Entry {
                        wire: alias.clone(),
                        protected: false,
                    },
                );
                if let Some(entry) = ids.get_mut(id) {
                    entry.wire = alias;
                }
            }
        }
        for message in messages {
            for id in result_ids(message) {
                let entry = &ids[id];
                if !entry.protected && !portable(&entry.wire) {
                    return Err(ProtocolError::InvalidToolCallId {
                        call_id: id.to_string(),
                    });
                }
            }
        }
        Ok(Self { ids })
    }

    /// The id `source` is sent under (`resolve`, `tool_call_ids.zig:96-99`).
    pub(crate) fn resolve<'a>(&'a self, source: &'a str) -> &'a str {
        self.ids
            .get(source)
            .map_or(source, |entry| entry.wire.as_str())
    }
}

/// Whether `id` goes on every wire as it is (`portable`,
/// `tool_call_ids.zig:102-109`).
pub(crate) fn portable(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The ids of the calls an assistant message makes, in order.
fn call_ids(message: &Message) -> impl Iterator<Item = &str> {
    message.content.iter().filter_map(|part| match part {
        ContentPart::ToolCall(call) => Some(call.id.as_str()),
        _ => None,
    })
}

/// The ids a tool message answers.
fn result_ids(message: &Message) -> impl Iterator<Item = &str> {
    message.content.iter().filter_map(|part| match part {
        ContentPart::ToolResult { call_id, .. } => Some(call_id.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::protocol::ToolCall;
    use serde_json::json;

    fn calls(ids: &[&str]) -> Message {
        Message::assistant(
            None,
            ids.iter()
                .map(|id| ToolCall {
                    id: (*id).to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                })
                .collect(),
        )
    }

    #[test]
    fn portable_ids_are_sent_as_they_are_and_nothing_is_computed() {
        // Upstream's first vector (`tool_call_ids.zig:120-129`): a call id of
        // letters, digits, `_` and `-`, and one of exactly 64 bytes.
        let long = "x".repeat(64);
        let messages = [
            Message::user("q"),
            calls(&["call_ABC-123", &long]),
            Message::tool_result("call_ABC-123", "read_file", "a"),
            Message::tool_result(long.clone(), "read_file", "b"),
        ];
        let projection = Projection::of(&messages, false).expect("projected");
        assert!(projection.ids.is_empty(), "{projection:?}");
        assert_eq!(projection.resolve("call_ABC-123"), "call_ABC-123");
        assert_eq!(projection.resolve(&long), long);
    }

    #[test]
    fn a_non_portable_id_is_sent_under_one_deterministic_alias_at_its_call_and_its_result() {
        // Upstream's third vector (`tool_call_ids.zig:139`): every source id
        // gets a portable alias, the same one each time, distinct from the
        // others, and the source is left as it was.
        let sources = [
            "functions.read:0".to_string(),
            "functions/read:0".to_string(),
            " ".to_string(),
            "\t\n".to_string(),
            "\u{e9}".to_string(),
            "x".repeat(65),
            "x".repeat(256),
        ];
        let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
        let mut messages = vec![Message::user("q"), calls(&refs)];
        messages.extend(
            sources
                .iter()
                .map(|id| Message::tool_result(id.clone(), "read_file", "r")),
        );
        let first = Projection::of(&messages, false).expect("projected");
        let second = Projection::of(&messages, true).expect("projected");
        let mut seen: Vec<&str> = Vec::new();
        for source in &sources {
            let id = first.resolve(source);
            assert!(portable(id), "{id:?} for {source:?}");
            assert_eq!(id, second.resolve(source), "not deterministic");
            assert!(!seen.contains(&id), "{id} is shared");
            seen.push(id);
        }
        // The alias: the prefix, 40 hex of the SHA-256, and the first step.
        let digest = Sha256::digest(b"functions.read:0");
        let hex: String = digest[..20].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(first.resolve("functions.read:0"), format!("fx_{hex}_0"));
        // fx's own request for the same call (`receipts/p1`).
        assert_eq!(
            first.resolve("functions.read:0"),
            "fx_c216cab2de2e56d547b4e6340edfc11a16bd7a07_0"
        );
    }

    #[test]
    fn an_alias_already_taken_steps_to_the_next() {
        // A portable id that happens to be the first alias of another one.
        let digest = Sha256::digest(b"a:1");
        let hex: String = digest[..20].iter().map(|b| format!("{b:02x}")).collect();
        let taken = format!("fx_{hex}_0");
        let messages = [
            Message::user("q"),
            calls(&[&taken, "a:1"]),
            Message::tool_result(taken.clone(), "read_file", "x"),
            Message::tool_result("a:1", "read_file", "y"),
        ];
        let projection = Projection::of(&messages, false).expect("projected");
        assert_eq!(projection.resolve(&taken), taken);
        assert_eq!(projection.resolve("a:1"), format!("fx_{hex}_1"));
    }

    #[test]
    fn a_call_in_blocks_the_wire_replays_is_never_renamed() {
        // llmux's own blocks name the call by the id it was minted with, so a
        // message that carries them keeps it -- and another message that
        // would need the same id renamed is refused rather than sent with
        // the two disagreeing (`ProtectedToolCallId`, `tool_call_ids.zig:56`).
        let call = ToolCall {
            id: "odd:id".to_string(),
            name: "read_file".to_string(),
            input: json!({}),
        };
        let replayed = Message::assistant_raw(
            vec![json!({"type": "tool_use", "id": "odd:id", "name": "read_file", "input": {}})],
            vec![call.clone()],
        );
        let messages = [
            Message::user("q"),
            replayed.clone(),
            Message::tool_result("odd:id", "read_file", "r"),
        ];
        let kept = Projection::of(&messages, true).expect("projected");
        assert_eq!(kept.resolve("odd:id"), "odd:id");
        // The Gateway sends the blocks' text only, so there it is renamed.
        let renamed = Projection::of(&messages, false).expect("projected");
        assert!(portable(renamed.resolve("odd:id")));

        let both = [
            Message::user("q"),
            replayed,
            Message::tool_result("odd:id", "read_file", "r"),
            Message::assistant(None, vec![call]),
        ];
        assert_eq!(
            Projection::of(&both, true).unwrap_err(),
            ProtocolError::ProtectedToolCallId {
                call_id: "odd:id".to_string()
            }
        );
    }

    #[test]
    fn an_empty_id_is_refused() {
        let messages = [Message::user("q"), calls(&[""])];
        assert!(matches!(
            Projection::of(&messages, false),
            Err(ProtocolError::InvalidToolCallId { .. })
        ));
    }
}
