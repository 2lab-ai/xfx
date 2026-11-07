//! A JSON-RPC message read as fx v0.0.16 reads one (`vercel-labs/fx@481104fd
//! src/mcp/protocol/wire.zig`): from its raw text, field by field, without
//! building a tree.
//!
//! What this decides and serde would not:
//! - a wanted field written twice is refused (`DuplicateField`), so a message
//!   cannot say two things at once;
//! - a key written with escapes is no key of the envelope, and a string
//!   written with escapes never matches one of the client's own strings
//!   (`plainString`, `arrayHasString`): `"2025\u002d06-18"` is no version;
//! - nesting stops at fx's scanner's stack ([`MAX_DEPTH`]);
//! - an id is the client's only as a non-negative integer, or a string of
//!   plain decimal digits (`ownId`, `decimal`).
//!
//! Every slice returned is a slice of the input.

/// The most containers -- objects and arrays -- one scan holds open at once.
/// fx's scanner keeps them in a 512-byte stack (`scan_stack_bytes`,
/// `wire.zig:100-101@481104fd`, "about 2,000 levels"); on fx v0.0.16's
/// release build a line of 2584 levels decodes and one of 2585 is `TooDeep`
/// (`p5-mcp-discover-stdio-depth`, `.prd/tui-phase5/receipts/mcp-discover/`).
pub const MAX_DEPTH: usize = 2584;

/// Why a message was refused (`DecodeError`, `wire.zig:82-98`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidJson,
    TooDeep,
    NotAnObject,
    NotAnArray,
    NotJsonRpc2,
    DuplicateField,
    /// The fields don't describe exactly one kind of message.
    Ambiguous,
    InvalidId,
    /// Not a string, or written with escapes.
    InvalidMethod,
    InvalidParams,
    InvalidResult,
    InvalidErrorObject,
}

impl DecodeError {
    /// What a refused reply says was wrong (`onRefusedLine`,
    /// `src/mcp/server.zig:777-783@481104fd`).
    pub fn why(self) -> &'static str {
        match self {
            Self::NotJsonRpc2 => "its jsonrpc isn't \"2.0\"",
            Self::Ambiguous => "it has both a result and an error, or neither",
            Self::InvalidResult => "its result isn't an object",
            Self::InvalidErrorObject => "its error isn't an error object",
            _ => "it isn't valid JSON-RPC",
        }
    }
}

/// Bytes of a line an excerpt keeps, before "..." (`excerpt_max`,
/// `src/mcp/server.zig:55-56@481104fd`).
pub const EXCERPT_MAX: usize = 80;

/// The message a refused reply fails its request with (`onRefusedLine`,
/// `src/mcp/server.zig:784-787@481104fd`): what was wrong, and the start of
/// the line as it came -- at most [`EXCERPT_MAX`] bytes, never a cut
/// character, every control byte `?`, and `...` when cut (`excerptOf`,
/// `:60-66`; `maskedPrefix`, `src/mcp/io/process.zig:170-177`).
pub fn invalid_reply(why: &str, line: &str) -> String {
    let mut end = line.len().min(EXCERPT_MAX);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let masked: String = line[..end]
        .chars()
        .map(|c| {
            if c < '\u{20}' || c == '\u{7f}' {
                '?'
            } else {
                c
            }
        })
        .collect();
    let cut = if end < line.len() { "..." } else { "" };
    format!("invalid reply ({why}): {masked}{cut}")
}

/// What a result says about itself (`ResultKind`, `wire.zig:52-60`): a
/// missing `resultType` means complete; an unknown one is unrecognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    Complete,
    InputRequired,
    Unrecognized,
}

/// A JSON-RPC error (`Failure`, `wire.zig:62-68`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure<'a> {
    pub code: i64,
    /// The message as a JSON string literal, with its quotes and escapes.
    pub message: &'a str,
    /// Raw JSON, when present.
    pub data: Option<&'a str>,
}

/// A response's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body<'a> {
    /// The whole result object, and its kind.
    Result {
        raw: &'a str,
        kind: ResultKind,
    },
    Failure(Failure<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response<'a> {
    /// The id when it is one the client could have issued; `None` when it is
    /// null, absent, negative, or a number or string that is not plain
    /// decimal digits -- so it can't be matched to a request.
    pub id: Option<u64>,
    pub body: Body<'a>,
}

/// One message (`Message`, `wire.zig:33-38`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message<'a> {
    Response(Response<'a>),
    Notification {
        method: &'a str,
        params: Option<&'a str>,
    },
    /// A request from the server; its raw id is echoed in the answer.
    Request {
        id: &'a str,
        method: &'a str,
        params: Option<&'a str>,
    },
}

const ENVELOPE: [&str; 6] = ["jsonrpc", "id", "method", "params", "result", "error"];

/// Decodes one complete message (`decode`, `wire.zig:106-131`).
pub fn decode(input: &str) -> Result<Message<'_>, DecodeError> {
    let [version, id, method, params, result, error] = object_fields(input, ENVELOPE)?;
    if version != Some("\"2.0\"") {
        return Err(DecodeError::NotJsonRpc2);
    }
    if params.is_some_and(|p| !p.starts_with('{')) {
        return Err(DecodeError::InvalidParams);
    }
    if let Some(method) = method {
        if result.is_some() || error.is_some() {
            return Err(DecodeError::Ambiguous);
        }
        let name = plain_string(method).ok_or(DecodeError::InvalidMethod)?;
        let Some(raw_id) = id else {
            return Ok(Message::Notification {
                method: name,
                params,
            });
        };
        if !is_request_id(raw_id) {
            return Err(DecodeError::InvalidId);
        }
        return Ok(Message::Request {
            id: raw_id,
            method: name,
            params,
        });
    }
    if params.is_some() || result.is_none() == error.is_none() {
        return Err(DecodeError::Ambiguous);
    }
    let response_id = own_id(id)?;
    if let Some(raw) = result {
        if !raw.starts_with('{') {
            return Err(DecodeError::InvalidResult);
        }
        return Ok(Message::Response(Response {
            id: response_id,
            body: Body::Result {
                raw,
                kind: result_kind(raw)?,
            },
        }));
    }
    Ok(Message::Response(Response {
        id: response_id,
        body: Body::Failure(failure_of(error.expect("one of the two"))?),
    }))
}

/// The id of a line [`decode`] refused that is shaped like a response: an
/// object with no `method` and an id the client could have issued. The
/// request it names fails with it (`responseId`, `wire.zig:132-140`).
pub fn response_id(input: &str) -> Option<u64> {
    let found = object_fields(input, ENVELOPE).ok()?;
    if found[2].is_some() {
        return None;
    }
    own_id(found[1]).ok().flatten()
}

/// The raw values of the fields `names` names in the JSON object `object`, in
/// the same order (`objectFields`, `wire.zig:142-174`). Other fields, and keys
/// written with escapes, are skipped; a wanted field that appears twice is an
/// error. The whole input must be one valid JSON object.
pub fn object_fields<'a, const N: usize>(
    object: &'a str,
    names: [&str; N],
) -> Result<[Option<&'a str>; N], DecodeError> {
    let mut out = [None; N];
    let mut s = Scanner::new(object);
    if s.first_token()? != b'{' {
        return Err(DecodeError::NotAnObject);
    }
    s.open()?;
    s.ws();
    if s.eat(b'}') {
        s.close();
    } else {
        loop {
            s.ws();
            let (key, escaped) = s.string()?;
            s.ws();
            if !s.eat(b':') {
                return Err(DecodeError::InvalidJson);
            }
            s.ws();
            let start = s.at;
            s.value()?;
            let raw = &object[start..s.at];
            if !escaped {
                for (name, slot) in names.iter().zip(out.iter_mut()) {
                    if *name == key {
                        if slot.is_some() {
                            return Err(DecodeError::DuplicateField);
                        }
                        *slot = Some(raw);
                    }
                }
            }
            s.ws();
            if s.eat(b',') {
                continue;
            }
            if s.eat(b'}') {
                s.close();
                break;
            }
            return Err(DecodeError::InvalidJson);
        }
    }
    s.end()?;
    Ok(out)
}

/// Whether the JSON array `array` holds the string `needle`. A string written
/// with escapes never matches, and other elements are skipped
/// (`arrayHasString`, `wire.zig:248-257`).
pub fn array_has_string(array: &str, needle: &str) -> Result<bool, DecodeError> {
    let mut found = false;
    for_each_element(array, |element| {
        if element.len() == needle.len() + 2
            && element.starts_with('"')
            && &element[1..element.len() - 1] == needle
        {
            found = true;
        }
    })?;
    Ok(found)
}

/// Calls `each` with every element of the JSON array `array`, raw
/// (`Elements`, `wire.zig:180-205`).
fn for_each_element<'a>(array: &'a str, mut each: impl FnMut(&'a str)) -> Result<(), DecodeError> {
    let mut s = Scanner::new(array);
    if s.first_token()? != b'[' {
        return Err(DecodeError::NotAnArray);
    }
    s.open()?;
    s.ws();
    if s.eat(b']') {
        s.close();
    } else {
        loop {
            s.ws();
            let start = s.at;
            s.value()?;
            each(&array[start..s.at]);
            s.ws();
            if s.eat(b',') {
                continue;
            }
            if s.eat(b']') {
                s.close();
                break;
            }
            return Err(DecodeError::InvalidJson);
        }
    }
    s.end()
}

/// The contents of a raw JSON string written without escapes, or `None` for
/// anything else (`plainString`, `wire.zig:274-279`).
pub fn plain_string(raw: &str) -> Option<&str> {
    if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
        return None;
    }
    let contents = &raw[1..raw.len() - 1];
    (!contents.contains('\\')).then_some(contents)
}

/// A raw JSON string's contents, unescaped, or `None` when it isn't a string
/// (`decodeString`, `wire.zig:281-298`).
pub fn decode_string(raw: &str) -> Option<String> {
    if !raw.starts_with('"') {
        return None;
    }
    serde_json::from_str::<String>(raw).ok()
}

/// JSON-RPC ids are strings or integers, never null (`isRequestId`,
/// `wire.zig:298-303`).
fn is_request_id(raw: &str) -> bool {
    let first = raw.as_bytes()[0];
    if first == b'"' {
        return true;
    }
    if first != b'-' && !first.is_ascii_digit() {
        return false;
    }
    !raw.contains(['.', 'e', 'E'])
}

/// The id of a response, when it is one the client could have issued
/// (`ownId`, `wire.zig:305-313`): a string's contents or a number's digits,
/// as plain decimal; `None` for null, absent, or negative; anything else is
/// an invalid id.
pub fn own_id(raw: Option<&str>) -> Result<Option<u64>, DecodeError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    match value.as_bytes()[0] {
        b'"' => Ok(decimal(&value[1..value.len() - 1])),
        b'0'..=b'9' => Ok(decimal(value)),
        b'-' | b'n' => Ok(None),
        _ => Err(DecodeError::InvalidId),
    }
}

/// Plain decimal digits without a leading zero, as the client writes its ids:
/// a non-negative integer that fits in 64 bits, as fx's `decimal` reads one
/// (`wire.zig:315-327`). fx's `max_request_id` (2^53 - 1, `:15-17`) bounds the
/// ids its request table issues (`IdsExhausted`, `src/mcp/core/request.zig:
/// 233`), not what this reads.
pub fn decimal(digits: &str) -> Option<u64> {
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A result's kind (`resultKind`, `wire.zig:328-335`): its `resultType` raw,
/// so an escaped `"complete"` is unrecognized.
fn result_kind(result: &str) -> Result<ResultKind, DecodeError> {
    let [found] = object_fields(result, ["resultType"])?;
    Ok(match found {
        None | Some("\"complete\"") => ResultKind::Complete,
        Some("\"input_required\"") => ResultKind::InputRequired,
        Some(_) => ResultKind::Unrecognized,
    })
}

/// An error object (`failureOf`, `wire.zig:337-350`): an integer `code` and
/// a string `message`.
fn failure_of(raw: &str) -> Result<Failure<'_>, DecodeError> {
    let [code, message, data] = object_fields(raw, ["code", "message", "data"]).map_err(|err| {
        if err == DecodeError::NotAnObject {
            DecodeError::InvalidErrorObject
        } else {
            err
        }
    })?;
    let code = code.ok_or(DecodeError::InvalidErrorObject)?;
    let message = message.ok_or(DecodeError::InvalidErrorObject)?;
    if !message.starts_with('"') {
        return Err(DecodeError::InvalidErrorObject);
    }
    let code = code
        .parse::<i64>()
        .map_err(|_| DecodeError::InvalidErrorObject)?;
    Ok(Failure {
        code,
        message,
        data,
    })
}

// ---------------------------------------------------------------------------
// the scanner
// ---------------------------------------------------------------------------

/// A strict RFC 8259 scanner over one complete input, holding at most
/// [`MAX_DEPTH`] containers open: no recursion, so no input can exhaust the
/// stack, and no allocation.
struct Scanner<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
    depth: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            text,
            at: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// What fx's first `nextToken` reads (`wire.zig:154`, `:190`): a
    /// container's bracket, left at the cursor, or a whole scalar token --
    /// whose kind is then all that matters; one that is not a token is
    /// invalid.
    fn first_token(&mut self) -> Result<u8, DecodeError> {
        self.ws();
        let first = self.peek().ok_or(DecodeError::InvalidJson)?;
        match first {
            b'{' | b'[' => {}
            b'"' => {
                self.string()?;
            }
            b't' => self.literal(b"true")?,
            b'f' => self.literal(b"false")?,
            b'n' => self.literal(b"null")?,
            b'-' | b'0'..=b'9' => self.number()?,
            _ => return Err(DecodeError::InvalidJson),
        }
        Ok(first)
    }

    /// Opens a container at the cursor.
    fn open(&mut self) -> Result<(), DecodeError> {
        if self.depth == MAX_DEPTH {
            return Err(DecodeError::TooDeep);
        }
        self.depth += 1;
        self.at += 1;
        Ok(())
    }

    fn close(&mut self) {
        self.depth -= 1;
    }

    /// Only whitespace is left.
    fn end(&mut self) -> Result<(), DecodeError> {
        self.ws();
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::InvalidJson)
        }
    }

    /// A string at the cursor: its raw contents, and whether it has escapes.
    fn string(&mut self) -> Result<(&'a str, bool), DecodeError> {
        if !self.eat(b'"') {
            return Err(DecodeError::InvalidJson);
        }
        let start = self.at;
        let mut escaped = false;
        loop {
            match self.peek() {
                None => return Err(DecodeError::InvalidJson),
                Some(b'"') => {
                    let contents = &self.text[start..self.at];
                    self.at += 1;
                    return Ok((contents, escaped));
                }
                Some(b'\\') => {
                    escaped = true;
                    self.at += 1;
                    self.escape()?;
                }
                Some(0x00..=0x1f) => return Err(DecodeError::InvalidJson),
                Some(_) => self.at += 1,
            }
        }
    }

    /// One escape after its backslash; a `\u` high surrogate must be followed
    /// by a low one, and a low one never comes alone.
    fn escape(&mut self) -> Result<(), DecodeError> {
        match self.peek() {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.at += 1;
                Ok(())
            }
            Some(b'u') => {
                self.at += 1;
                let unit = self.hex4()?;
                if (0xdc00..=0xdfff).contains(&unit) {
                    return Err(DecodeError::InvalidJson);
                }
                if (0xd800..=0xdbff).contains(&unit) {
                    if !(self.eat(b'\\') && self.eat(b'u')) {
                        return Err(DecodeError::InvalidJson);
                    }
                    let low = self.hex4()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(DecodeError::InvalidJson);
                    }
                }
                Ok(())
            }
            _ => Err(DecodeError::InvalidJson),
        }
    }

    fn hex4(&mut self) -> Result<u32, DecodeError> {
        let digits = self
            .text
            .get(self.at..self.at + 4)
            .filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(DecodeError::InvalidJson)?;
        self.at += 4;
        u32::from_str_radix(digits, 16).map_err(|_| DecodeError::InvalidJson)
    }

    fn number(&mut self) -> Result<(), DecodeError> {
        self.eat(b'-');
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(DecodeError::InvalidJson),
        }
        if self.eat(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(DecodeError::InvalidJson);
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(DecodeError::InvalidJson);
            }
            self.digits();
        }
        Ok(())
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), DecodeError> {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(DecodeError::InvalidJson)
        }
    }

    /// Skips one whole value at the cursor, nested containers included,
    /// without recursing: the open containers are counted, and each closes
    /// where its own kind of bracket says.
    fn value(&mut self) -> Result<(), DecodeError> {
        // Kinds of the containers this value opened, innermost last; their
        // count is bounded by MAX_DEPTH.
        let mut kinds: Vec<u8> = Vec::new();
        loop {
            // A value.
            self.ws();
            match self.peek() {
                Some(b'{') => {
                    self.open()?;
                    kinds.push(b'}');
                    self.ws();
                    if self.eat(b'}') {
                        self.close();
                        kinds.pop();
                    } else {
                        self.member_key()?;
                        continue;
                    }
                }
                Some(b'[') => {
                    self.open()?;
                    kinds.push(b']');
                    self.ws();
                    if self.eat(b']') {
                        self.close();
                        kinds.pop();
                    } else {
                        continue;
                    }
                }
                Some(b'"') => {
                    self.string()?;
                }
                Some(b't') => self.literal(b"true")?,
                Some(b'f') => self.literal(b"false")?,
                Some(b'n') => self.literal(b"null")?,
                Some(b'-' | b'0'..=b'9') => self.number()?,
                _ => return Err(DecodeError::InvalidJson),
            }
            // After a value: close what ends here, or go on to the next one.
            loop {
                let Some(&kind) = kinds.last() else {
                    return Ok(());
                };
                self.ws();
                if self.eat(b',') {
                    if kind == b'}' {
                        self.ws();
                        self.member_key()?;
                    }
                    break;
                }
                if self.eat(kind) {
                    self.close();
                    kinds.pop();
                    continue;
                }
                return Err(DecodeError::InvalidJson);
            }
        }
    }

    /// A member's key and its colon.
    fn member_key(&mut self) -> Result<(), DecodeError> {
        self.string()?;
        self.ws();
        if self.eat(b':') {
            Ok(())
        } else {
            Err(DecodeError::InvalidJson)
        }
    }
}

#[cfg(test)]
mod tests;
