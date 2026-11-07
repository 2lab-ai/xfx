//! Which MCP a server speaks: fx v0.0.16's version detection core, ported
//! (`vercel-labs/fx@481104fd src/mcp/core/era.zig`).
//!
//! A start asks `server/discover` with the newest version first -- the
//! 2026-07-28 era -- and a server that answers it with a version list is
//! served in that era, with no `initialize` at all; a server that does not
//! know the method, answers with any other error, or says nothing within the
//! probe's 10 s (cut to what is left of the start) is a 2025 server, and gets
//! the legacy `initialize`, which may agree on `2025-11-25`, `2025-06-18` or
//! `2025-03-26` and nothing older (`Config`, `era.zig:19-26`).
//!
//! [`Detector`] is the pure core: events are what the client saw, effects
//! what it must send next, and nothing in it does I/O, reads a clock or keeps
//! a string a server sent -- a server's version list arrives as a [`Listed`]
//! mask over the client's own versions, so a hostile list has nowhere to go
//! but that mask. The `classify_*` functions turn an answer into an event, and
//! [`meta`] writes the `_meta` a request carries. Answers are read from their
//! raw text, as fx reads them ([`super::wire`]).
//!
//! The runtime drives detection only (`super::runtime`, one blocking exchange
//! at a time): [`Event::OperationSent`], [`Event::OperationVersionRejected`]
//! and [`classify_operation_rejection`] -- an operation's version switch on
//! -32022 -- are here, and tested with the rest of the core, but nothing
//! sends them yet. That is MCP-E2's, with the rest of what fx does once a
//! 2026 server is ready.

use std::time::Duration;

use serde_json::{Map, Value};

use super::wire;

/// The most modern versions a client may prefer (`max_preferred`).
pub const MAX_PREFERRED: usize = 8;

/// Where detection is (`State`, `era.zig:17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Unknown,
    Probing,
    Initializing,
    Modern,
    Legacy,
    Failed,
}

/// The client's versions (`Config`, `era.zig:19-26`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Modern versions, most preferred first.
    pub preferred: &'static [&'static str],
    /// The 2025-era versions `initialize` may agree on, newest first; the
    /// first is the one offered.
    pub legacy: &'static [&'static str],
    /// How long a probe waits for its answer, before what is left of the
    /// start cuts it.
    pub probe_timeout: Duration,
}

impl Config {
    /// fx v0.0.16's: `2026-07-28`, then the three 2025 versions, and a 10 s
    /// probe (`era.zig:21-25`). `2024-11-05` is gone: "fx speaks 2025-03-26
    /// and newer" (`src/core/mcp_host/runtime.zig:2296@481104fd`).
    pub const FX: Self = Self {
        preferred: &["2026-07-28"],
        legacy: &["2025-11-25", "2025-06-18", "2025-03-26"],
        probe_timeout: Duration::from_secs(10),
    };
}

impl Default for Config {
    fn default() -> Self {
        Self::FX
    }
}

/// Which of the client's versions a server listed (`Listed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Listed {
    /// Bit i stands for `Config::preferred[i]`.
    pub preferred: u8,
    pub legacy: bool,
}

impl Listed {
    pub fn has(self, index: usize) -> bool {
        self.preferred & (1u8 << index) != 0
    }
}

/// A version as the core sees it: one of the client's, by index, or another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    None,
    /// Index into `Config::legacy`.
    Legacy(u8),
    /// Index into `Config::preferred`.
    Preferred(u8),
    Other,
}

/// The version string for `version`, if it is one of the client's
/// (`versionName`).
pub fn version_name(config: &Config, version: Version) -> Option<&'static str> {
    match version {
        Version::Legacy(i) => config.legacy.get(usize::from(i)).copied(),
        Version::Preferred(i) => config.preferred.get(usize::from(i)).copied(),
        Version::None | Version::Other => None,
    }
}

/// Why detection failed (`Failure`, `era.zig:58-69`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The server lists no version the client supports.
    NoSharedVersion,
    /// -32020 or -32021: a modern server that won't serve this client.
    Rejected,
    /// A server that answered like a modern one stopped answering.
    WentQuiet,
    /// `initialize` was answered with a version other than a legacy one.
    UnsupportedVersion,
    /// `initialize` was rejected, or never answered.
    InitializeFailed,
}

impl Failure {
    /// The error's name, which is how the runtime carries it.
    pub fn name(self) -> &'static str {
        match self {
            Self::NoSharedVersion => "McpNoSharedVersion",
            Self::Rejected => "McpVersionRejected",
            Self::WentQuiet => "McpWentQuiet",
            Self::UnsupportedVersion => "McpUnsupportedProtocolVersion",
            Self::InitializeFailed => "McpInitializeFailed",
        }
    }

    /// The failure an error name carries, if it is one of these.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::NoSharedVersion,
            Self::Rejected,
            Self::WentQuiet,
            Self::UnsupportedVersion,
            Self::InitializeFailed,
        ]
        .into_iter()
        .find(|failure| failure.name() == name)
    }

    /// fx's words for it, with xfx's name (`describeFailure`,
    /// `src/core/mcp_host/runtime.zig:2290-2297@481104fd`).
    pub fn words(self) -> &'static str {
        match self {
            Self::NoSharedVersion => "it supports no MCP version xfx speaks",
            Self::Rejected => "it refused xfx's MCP version",
            Self::WentQuiet => "it stopped answering while starting",
            Self::UnsupportedVersion => {
                "it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)"
            }
            Self::InitializeFailed => "it refused or never answered xfx's initialize request",
        }
    }
}

/// What the client saw (`Event`, `era.zig:71-96`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A new server process is running (over HTTP: detection starts over).
    ProcessStarted,
    DetectRequested,
    DiscoverResult(Listed),
    /// The probe got -32022.
    UnsupportedVersion(Listed),
    /// The probe got -32020 or -32021.
    ModernRejection,
    /// The probe got any other error, or an error without an id.
    LegacyAnswer,
    /// HTTP only: the probe got 404 with -32601, from a modern server that
    /// doesn't implement discovery. It is served with the probe's version.
    DiscoverNotImplemented,
    /// The probe's timer. A timer from an earlier probe changes nothing.
    ProbeTimerFired(u32),
    /// The version `initialize` was answered with.
    InitializeResult(Version),
    /// `initialize` was rejected; true when the error hints at a modern server.
    InitializeError(bool),
    InitializeTimedOut,
    /// An operation request is about to go out; `Output::meta` says what it
    /// carries.
    OperationSent,
    /// An operation request got -32022.
    OperationVersionRejected(Listed),
}

/// What the client must do next (`Effect`, `era.zig:98-107`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Send `server/discover` with `Config::preferred[index]`.
    SendDiscover(u8),
    SendInitialize,
    SendInitialized,
    ArmProbeTimer {
        probe: u32,
        after: Duration,
    },
    /// Requests may go out, on a modern server with this version or on a
    /// legacy one.
    Ready(Version),
    Failed(Failure),
}

/// The version the last operation request carried (`Meta`, for the model's
/// variables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastMeta {
    Unset,
    None,
    Preferred(u8),
}

/// What is outstanding (`Pending`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    None,
    Discover,
    Initialize,
}

/// One step's result (`Output`, `era.zig:141-158`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    pub effects: Vec<Effect>,
    /// For `OperationSent`: the version the request carries.
    pub meta: Option<Version>,
    /// False for a timer from an earlier probe, which is no model step.
    pub stepped: bool,
    /// The event isn't enabled in the model here, so nothing changed.
    pub ignored: bool,
}

impl Output {
    fn push(&mut self, effect: Effect) {
        debug_assert!(self.effects.len() < 2, "at most two effects a step");
        self.effects.push(effect);
    }
}

/// Operation requests wait until detection finishes (`error.NotReady`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotReady;

/// The detection core (`Detector`, `era.zig:165-367`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detector {
    pub config: Config,
    pub state: State,
    pub version: Version,
    tried: [u8; MAX_PREFERRED],
    pub pending: Pending,
    advertised: Listed,
    process: u32,
    last_meta: LastMeta,
    modern_seen: bool,
    /// Counts probes, so a timer from an earlier one is recognized.
    pub probe: u32,
    pub failure: Option<Failure>,
}

impl Detector {
    pub fn new(config: Config) -> Self {
        assert!(
            !config.preferred.is_empty() && config.preferred.len() <= MAX_PREFERRED,
            "one to eight preferred versions"
        );
        Self {
            config,
            state: State::Unknown,
            version: Version::None,
            tried: [0; MAX_PREFERRED],
            pending: Pending::None,
            advertised: Listed::default(),
            process: 0,
            last_meta: LastMeta::Unset,
            modern_seen: false,
            probe: 0,
            failure: None,
        }
    }

    pub fn ready(&self) -> bool {
        matches!(self.state, State::Modern | State::Legacy)
    }

    /// Applies one event (`step`, `era.zig:189-308`). An error leaves the
    /// detector unchanged.
    pub fn step(&mut self, event: Event) -> Result<Output, NotReady> {
        let mut out = Output {
            stepped: true,
            ..Output::default()
        };
        let mut ignored = false;
        match event {
            Event::ProcessStarted => {
                let (process, probe) = (self.process + 1, self.probe);
                *self = Self::new(self.config);
                self.process = process;
                self.probe = probe;
            }
            Event::DetectRequested => {
                if self.state == State::Unknown && self.process > 0 {
                    self.start_probe(0, &mut out);
                } else {
                    ignored = true;
                }
            }
            Event::DiscoverResult(listed) => {
                if self.pending != Pending::Discover {
                    ignored = true;
                } else {
                    self.advertised = listed;
                    self.modern_seen = true;
                    if let Some(index) = self.first_shared(listed) {
                        self.become_ready(Version::Preferred(index), &mut out);
                    } else if listed.legacy {
                        self.start_initialize(&mut out);
                    } else {
                        self.fail(Failure::NoSharedVersion, &mut out);
                    }
                }
            }
            Event::UnsupportedVersion(listed) => {
                if self.pending != Pending::Discover {
                    ignored = true;
                } else {
                    self.advertised = listed;
                    self.modern_seen = true;
                    if let Some(index) = self.next_candidate(listed) {
                        self.start_probe(index, &mut out);
                    } else if listed.legacy {
                        self.start_initialize(&mut out);
                    } else {
                        self.fail(Failure::NoSharedVersion, &mut out);
                    }
                }
            }
            Event::ModernRejection => {
                if self.pending != Pending::Discover {
                    ignored = true;
                } else {
                    self.modern_seen = true;
                    self.fail(Failure::Rejected, &mut out);
                }
            }
            Event::LegacyAnswer => {
                if self.pending == Pending::Discover {
                    self.legacy_answer(&mut out);
                } else {
                    ignored = true;
                }
            }
            Event::DiscoverNotImplemented => {
                if self.pending != Pending::Discover {
                    ignored = true;
                } else {
                    self.modern_seen = true;
                    self.become_ready(self.version, &mut out);
                }
            }
            Event::ProbeTimerFired(probe) => {
                // A timer from an earlier probe, or for one already answered,
                // is no step.
                if probe != self.probe || self.pending != Pending::Discover {
                    return Ok(Output::default());
                }
                self.legacy_answer(&mut out);
            }
            Event::InitializeResult(version) => {
                if self.pending != Pending::Initialize {
                    ignored = true;
                } else if matches!(version, Version::Legacy(_)) {
                    out.push(Effect::SendInitialized);
                    self.become_ready(version, &mut out);
                } else {
                    self.fail(Failure::UnsupportedVersion, &mut out);
                }
            }
            Event::InitializeError(hint) => {
                if self.pending != Pending::Initialize {
                    ignored = true;
                } else if hint && self.tried[0] < 2 {
                    // A modern server that missed the probe; probe once more.
                    // Detection always starts with the first preferred
                    // version, so its two-probe limit allows exactly one fall
                    // forward.
                    self.modern_seen = true;
                    self.start_probe(0, &mut out);
                } else {
                    self.fail(Failure::InitializeFailed, &mut out);
                }
            }
            Event::InitializeTimedOut => {
                if self.pending == Pending::Initialize {
                    self.fail(Failure::InitializeFailed, &mut out);
                } else {
                    ignored = true;
                }
            }
            Event::OperationSent => {
                if !self.ready() {
                    return Err(NotReady);
                }
                self.last_meta = match (self.state, self.version) {
                    (State::Modern, Version::Preferred(i)) => LastMeta::Preferred(i),
                    _ => LastMeta::None,
                };
                out.meta = Some(if self.state == State::Modern {
                    self.version
                } else {
                    Version::None
                });
            }
            Event::OperationVersionRejected(listed) => {
                if self.state != State::Modern {
                    ignored = true;
                } else {
                    self.advertised = listed;
                    // Never switch eras mid-process.
                    if let Some(index) = self.next_candidate(listed) {
                        self.tried[usize::from(index)] += 1;
                        self.version = Version::Preferred(index);
                        self.last_meta = LastMeta::Unset;
                        out.push(Effect::Ready(self.version));
                    } else {
                        self.fail(Failure::NoSharedVersion, &mut out);
                    }
                }
            }
        }
        out.ignored = ignored;
        Ok(out)
    }

    /// The first preferred version the server listed and hasn't been tried;
    /// else the current version, tried once, if listed (a server that rejects
    /// a version it lists gets one retry).
    fn next_candidate(&self, listed: Listed) -> Option<u8> {
        for i in 0..self.config.preferred.len() {
            if listed.has(i) && self.tried[i] == 0 {
                return u8::try_from(i).ok();
            }
        }
        match self.version {
            Version::Preferred(i)
                if listed.has(usize::from(i)) && self.tried[usize::from(i)] == 1 =>
            {
                Some(i)
            }
            _ => None,
        }
    }

    fn first_shared(&self, listed: Listed) -> Option<u8> {
        (0..self.config.preferred.len())
            .find(|&i| listed.has(i))
            .and_then(|i| u8::try_from(i).ok())
    }

    fn start_probe(&mut self, index: u8, out: &mut Output) {
        self.state = State::Probing;
        self.version = Version::Preferred(index);
        self.tried[usize::from(index)] += 1;
        self.pending = Pending::Discover;
        self.probe += 1;
        out.push(Effect::SendDiscover(index));
        out.push(Effect::ArmProbeTimer {
            probe: self.probe,
            after: self.config.probe_timeout,
        });
    }

    /// Fall back to `initialize`.
    fn start_initialize(&mut self, out: &mut Output) {
        self.state = State::Initializing;
        self.version = Version::Legacy(0);
        self.pending = Pending::Initialize;
        out.push(Effect::SendInitialize);
    }

    /// The server is legacy, unless it already answered like a modern one in
    /// this process and didn't list the legacy version.
    fn legacy_answer(&mut self, out: &mut Output) {
        if self.modern_seen && !self.advertised.legacy {
            self.fail(Failure::WentQuiet, out);
        } else {
            self.start_initialize(out);
        }
    }

    fn become_ready(&mut self, version: Version, out: &mut Output) {
        self.state = if matches!(version, Version::Legacy(_)) {
            State::Legacy
        } else {
            State::Modern
        };
        self.version = version;
        self.pending = Pending::None;
        out.push(Effect::Ready(version));
    }

    fn fail(&mut self, failure: Failure, out: &mut Output) {
        self.state = State::Failed;
        self.pending = Pending::None;
        self.failure = Some(failure);
        out.push(Effect::Failed(failure));
    }
}

// ---------------------------------------------------------------------------
// answers
// ---------------------------------------------------------------------------

/// The JSON-RPC error codes the detection reads (`code`, `wire.zig:20-31`).
pub mod code {
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const HEADER_MISMATCH: i64 = -32020;
    pub const MISSING_REQUIRED_CLIENT_CAPABILITY: i64 = -32021;
    pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
}

/// The client versions a raw JSON array of strings lists (`listedIn`,
/// `era.zig:370-377`): only a string written exactly as the client's own,
/// without escapes, counts, and other elements are skipped
/// ([`wire::array_has_string`]); a string is only ever compared, never kept.
pub fn listed_in(config: &Config, array: &str) -> Result<Listed, wire::DecodeError> {
    let mut listed = Listed::default();
    for version in config.legacy {
        // `or`, as fx's: once one is found the rest are not looked at.
        listed.legacy = listed.legacy || wire::array_has_string(array, version)?;
    }
    for (i, version) in config.preferred.iter().enumerate() {
        if wire::array_has_string(array, version)? {
            listed.preferred |= 1u8 << i;
        }
    }
    Ok(listed)
}

/// The versions in the `supported` list of a -32022 error's data, if any
/// (`supportedIn`, `era.zig:380-384`).
fn supported_in(config: &Config, data: Option<&str>) -> Listed {
    let Some(data) = data else {
        return Listed::default();
    };
    let Ok([Some(supported)]) = wire::object_fields(data, ["supported"]) else {
        return Listed::default();
    };
    listed_in(config, supported).unwrap_or_default()
}

/// The event for the answer to the current probe (`classifyDiscover`,
/// `era.zig:391-405`). A result without a `supportedVersions` list isn't a
/// DiscoverResult, so it counts as legacy, like every error that isn't a
/// recognized modern one. So does a result whose `resultType` isn't
/// complete.
pub fn classify_discover(config: &Config, body: wire::Body<'_>) -> Event {
    match body {
        wire::Body::Result { raw, kind } => {
            if kind != wire::ResultKind::Complete {
                return Event::LegacyAnswer;
            }
            let Ok([Some(list)]) = wire::object_fields(raw, ["supportedVersions"]) else {
                return Event::LegacyAnswer;
            };
            match listed_in(config, list) {
                Ok(listed) => Event::DiscoverResult(listed),
                Err(_) => Event::LegacyAnswer,
            }
        }
        wire::Body::Failure(failure) => match failure.code {
            code::UNSUPPORTED_PROTOCOL_VERSION => {
                Event::UnsupportedVersion(supported_in(config, failure.data))
            }
            code::HEADER_MISMATCH | code::MISSING_REQUIRED_CLIENT_CAPABILITY => {
                Event::ModernRejection
            }
            _ => Event::LegacyAnswer,
        },
    }
}

/// The event for an HTTP answer to the current probe: its status, and the
/// JSON-RPC message in its body, if any (`classifyDiscoverHttp`,
/// `era.zig:411-415`). A 404 with -32601 is a modern server without
/// discovery; a body that isn't a JSON-RPC message counts as legacy.
pub fn classify_discover_http(config: &Config, status: u16, body: Option<wire::Body<'_>>) -> Event {
    let Some(body) = body else {
        return Event::LegacyAnswer;
    };
    if status == 404
        && matches!(
            body,
            wire::Body::Failure(wire::Failure {
                code: code::METHOD_NOT_FOUND,
                ..
            })
        )
    {
        return Event::DiscoverNotImplemented;
    }
    classify_discover(config, body)
}

/// The event for the answer to `initialize` (`classifyInitialize`,
/// `era.zig:420-444`). Its `protocolVersion` must be written plainly: one
/// written with escapes is another version (`plainString`). An error hints
/// at a modern server when it is a recognized modern error, or names a
/// preferred version in its supported list or message.
pub fn classify_initialize(config: &Config, body: wire::Body<'_>) -> Event {
    match body {
        wire::Body::Result { raw, .. } => {
            let version = match wire::object_fields(raw, ["protocolVersion"]) {
                Ok([Some(found)]) => wire::plain_string(found),
                _ => None,
            };
            let Some(version) = version else {
                return Event::InitializeResult(Version::Other);
            };
            if let Some(i) = config.legacy.iter().position(|v| *v == version) {
                return Event::InitializeResult(Version::Legacy(index(i)));
            }
            if let Some(i) = config.preferred.iter().position(|v| *v == version) {
                return Event::InitializeResult(Version::Preferred(index(i)));
            }
            Event::InitializeResult(Version::Other)
        }
        wire::Body::Failure(failure) => {
            let modern_code = matches!(
                failure.code,
                code::UNSUPPORTED_PROTOCOL_VERSION
                    | code::HEADER_MISMATCH
                    | code::MISSING_REQUIRED_CLIENT_CAPABILITY
            );
            let named = supported_in(config, failure.data).preferred != 0
                || config.preferred.iter().any(|v| failure.message.contains(v));
            Event::InitializeError(modern_code || named)
        }
    }
}

/// The event for -32022 in reply to an operation request
/// (`classifyOperationRejection`, `era.zig:447-450`). Not driven yet: an
/// operation's version switch is MCP-E2's.
pub fn classify_operation_rejection(config: &Config, failure: wire::Failure<'_>) -> Option<Event> {
    (failure.code == code::UNSUPPORTED_PROTOCOL_VERSION)
        .then(|| Event::OperationVersionRejected(supported_in(config, failure.data)))
}

fn index(i: usize) -> u8 {
    u8::try_from(i).expect("at most eight versions")
}

// ---------------------------------------------------------------------------
// what a request carries
// ---------------------------------------------------------------------------

/// What a request's `_meta` says beside its progress token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meta {
    /// A 2025 request, or one before detection: the progress token only.
    Legacy,
    /// A 2026 request: its version, the client's capabilities and who it is.
    Modern(&'static str),
}

/// The capabilities xfx declares: none. It answers no elicitation, sampling
/// or roots, so it advertises none of them (fx declares the elicitation it
/// answers, `declaredCapabilities`, `src/core/mcp_host/runtime.zig:2220-2226
/// @481104fd`).
pub fn client_capabilities() -> Value {
    Value::Object(Map::new())
}

/// Who xfx is, in `initialize`'s `clientInfo` and a 2026 request's `_meta`.
pub fn client_info() -> Value {
    let mut info = Map::new();
    info.insert("name".into(), Value::from("xfx"));
    info.insert("version".into(), Value::from(env!("CARGO_PKG_VERSION")));
    Value::Object(info)
}

/// The `_meta` for request `id` (`meta`, `era.zig:462-474`): the progress
/// token, which is the id, and for a 2026 request the version, the
/// capabilities and the client, in fx's order.
pub fn meta(meta: &Meta, id: u64) -> Value {
    let mut fields = Map::new();
    fields.insert("progressToken".into(), Value::from(id));
    if let Meta::Modern(version) = meta {
        fields.insert(
            "io.modelcontextprotocol/protocolVersion".into(),
            Value::from(*version),
        );
        fields.insert(
            "io.modelcontextprotocol/clientCapabilities".into(),
            client_capabilities(),
        );
        fields.insert("io.modelcontextprotocol/clientInfo".into(), client_info());
    }
    Value::Object(fields)
}

/// `initialize`'s params beside `_meta`: the newest legacy version, the
/// client's capabilities and who it is (`writeInitialize`, `era.zig:482-491`).
pub fn initialize_params(config: &Config) -> Value {
    let mut params = Map::new();
    params.insert("protocolVersion".into(), Value::from(config.legacy[0]));
    params.insert("capabilities".into(), client_capabilities());
    params.insert("clientInfo".into(), client_info());
    Value::Object(params)
}

#[cfg(test)]
mod tests;
