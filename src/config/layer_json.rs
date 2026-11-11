//! A settings layer's text as fx's parse reads it: `std.json.parseFromSlice`
//! into a `std.json.Value` with the default options (the profile,
//! `settings_store.zig:408@481104fd`; the project file,
//! `config_runtime.zig:539`). That parse has no depth limit -- it keeps its
//! containers on an allocated stack, never the call stack
//! (`Value.jsonParse`, `std/json/dynamic.zig:76-124@0.17.0`; the scanner's
//! `BitStack`, `Scanner.zig:63-67`) -- and no number it refuses: one past
//! `i64` or past `f64` is kept as its text, `.number_string`
//! (`parseFromNumberSlice`, `dynamic.zig:30-47`), which no settings key fx
//! reads takes. It refuses a key twice in one object, at any depth and as
//! decoded (`duplicate_field_behavior = .@"error"`, `static.zig:22-26`).
//! serde_json refuses both 128 containers deep and a number past `f64`, so
//! the layer is read here, without recursion, into what serde_json would
//! have made of it.

use serde_json::{Map, Value};
use std::collections::HashSet;

/// Containers one inside another, the settings object counted, that are
/// kept: serde_json's own bound (it reads 127 and refuses 128), so a file
/// serde_json reads is read into the same value. A container deeper still is
/// read through -- its syntax, and no key twice -- and kept as an empty one
/// of its kind: no key xfx reads is that deep, and a value nested no deeper
/// than this is one every recursive walk of a `Value` (its drop among them)
/// already takes.
const KEPT_DEPTH: usize = 127;

/// The object `text` holds, or `None` when fx's parse refuses it or it holds
/// some other value.
pub(super) fn object(text: &str) -> Option<Map<String, Value>> {
    match value(text)? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

/// One container being read: what is kept of it, and an object's keys so
/// far, decoded.
enum Open {
    Array(Vec<Value>),
    Object {
        fields: Map<String, Value>,
        keys: HashSet<String>,
        key: String,
    },
}

/// The one JSON value `text` holds, in RFC 8259's grammar as `std.json`'s
/// scanner reads it, or `None`. Strings and numbers are decoded by
/// serde_json, a token at a time, so each is the value serde_json's parse of
/// the whole file would have given -- but a number past `f64`, which is kept
/// as one no reader takes ([`number`]).
fn value(text: &str) -> Option<Value> {
    let mut reader = Reader { text, at: 0 };
    let mut open: Vec<Open> = Vec::new();
    loop {
        // A value is due here.
        reader.skip_space();
        let mut done = match reader.peek()? {
            b'{' => {
                reader.at += 1;
                reader.skip_space();
                if reader.eat(b'}') {
                    Value::Object(Map::new())
                } else {
                    let key = reader.key()?;
                    let keys = HashSet::from([key.clone()]);
                    let fields = Map::new();
                    open.push(Open::Object { fields, keys, key });
                    continue;
                }
            }
            b'[' => {
                reader.at += 1;
                reader.skip_space();
                if reader.eat(b']') {
                    Value::Array(Vec::new())
                } else {
                    open.push(Open::Array(Vec::new()));
                    continue;
                }
            }
            _ => reader.scalar()?,
        };
        // `done` is whole: give it to the container it is in, and close each
        // container that ends with it.
        loop {
            let kept = open.len() <= KEPT_DEPTH;
            match open.last_mut() {
                None => {
                    reader.skip_space();
                    return (reader.at == text.len()).then_some(done);
                }
                Some(Open::Array(items)) => {
                    if kept {
                        items.push(done);
                    }
                    reader.skip_space();
                    if reader.eat(b',') {
                        break;
                    }
                    if !reader.eat(b']') {
                        return None;
                    }
                }
                Some(Open::Object { fields, keys, key }) => {
                    if kept {
                        fields.insert(std::mem::take(key), done);
                    }
                    reader.skip_space();
                    if reader.eat(b',') {
                        reader.skip_space();
                        *key = reader.key()?;
                        if !keys.insert(key.clone()) {
                            return None;
                        }
                        break;
                    }
                    if !reader.eat(b'}') {
                        return None;
                    }
                }
            }
            done = match open.pop()? {
                Open::Array(items) => Value::Array(items),
                Open::Object { fields, .. } => Value::Object(fields),
            };
        }
    }
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn word(&mut self, word: &str) -> Option<()> {
        let found = self.text.as_bytes()[self.at..].starts_with(word.as_bytes());
        found.then(|| self.at += word.len())
    }

    /// An object's key and the `:` after it.
    fn key(&mut self) -> Option<String> {
        if self.peek()? != b'"' {
            return None;
        }
        let key = self.string()?;
        self.skip_space();
        self.eat(b':').then_some(key)
    }

    fn scalar(&mut self) -> Option<Value> {
        match self.peek()? {
            b'"' => self.string().map(Value::String),
            b't' => self.word("true").map(|()| Value::Bool(true)),
            b'f' => self.word("false").map(|()| Value::Bool(false)),
            b'n' => self.word("null").map(|()| Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    /// The string token here, decoded by serde_json: its escapes, its
    /// surrogate pairs and the control bytes it refuses are serde_json's.
    /// The token ends at the first quote no backslash escapes; both ends are
    /// ASCII quotes, so the slice is on character boundaries.
    fn string(&mut self) -> Option<String> {
        let bytes = self.text.as_bytes();
        let start = self.at;
        let mut at = start + 1;
        loop {
            match *bytes.get(at)? {
                b'"' => break,
                b'\\' => at += 2,
                _ => at += 1,
            }
        }
        self.at = at + 1;
        serde_json::from_str(&self.text[start..self.at]).ok()
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`, as serde_json
    /// would read it -- or, past `f64`'s range, which serde_json refuses and
    /// `std.json` keeps as its text (`.number_string`), the largest `f64` of
    /// its sign: a number and no integer, which every key xfx reads refuses as
    /// fx's refuses the text (`value != .integer`, `config_runtime.zig:1936`;
    /// `context_limits.zig:193-199`).
    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.at += 1;
        }
        let token = &self.text[start..self.at];
        if !is_number(token.as_bytes()) {
            return None;
        }
        Some(serde_json::from_str(token).unwrap_or_else(|_| {
            Value::from(if token.starts_with('-') {
                -f64::MAX
            } else {
                f64::MAX
            })
        }))
    }
}

/// Whether `token` is all one JSON number (RFC 8259 section 6).
fn is_number(token: &[u8]) -> bool {
    let digit = |at: usize| token.get(at).is_some_and(u8::is_ascii_digit);
    let mut at = usize::from(token.first() == Some(&b'-'));
    match token.get(at) {
        Some(b'0') => at += 1,
        Some(b'1'..=b'9') => {
            while digit(at) {
                at += 1;
            }
        }
        _ => return false,
    }
    if token.get(at) == Some(&b'.') {
        at += 1;
        if !digit(at) {
            return false;
        }
        while digit(at) {
            at += 1;
        }
    }
    if matches!(token.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(token.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        if !digit(at) {
            return false;
        }
        while digit(at) {
            at += 1;
        }
    }
    at == token.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{"zzz": ...}` with `depth` containers, the object counted: arrays, or
    /// objects each holding the next under `"a"`.
    fn nested(depth: usize, objects: bool) -> String {
        let inner = depth - 1;
        if objects {
            format!(
                r#"{{"zzz": {}0{}}}"#,
                r#"{"a": "#.repeat(inner),
                "}".repeat(inner)
            )
        } else {
            format!(r#"{{"zzz": {}{}}}"#, "[".repeat(inner), "]".repeat(inner))
        }
    }

    /// How many containers `value` holds one inside another, itself counted:
    /// walked with a stack of its own, so a deep one cannot overflow this.
    fn depth_of(value: &Value) -> usize {
        let mut deepest = 0;
        let mut stack = vec![(value, 1)];
        while let Some((value, depth)) = stack.pop() {
            let children: Vec<&Value> = match value {
                Value::Array(items) => items.iter().collect(),
                Value::Object(fields) => fields.values().collect(),
                _ => continue,
            };
            deepest = deepest.max(depth);
            stack.extend(children.into_iter().map(|child| (child, depth + 1)));
        }
        deepest
    }

    /// fx reads a settings file at any depth (`dynamic.zig:76-124@0.17.0`):
    /// 128 and 129 deep, 1,000, the deepest 64 KiB holds, and 100,000 -- on
    /// a thread with the default stack, which a recursive reader would run
    /// out of long before. What is kept is serde_json's 127 and the empty
    /// container in its place: nothing deeper is built, so nothing deeper is
    /// walked again (a drop, a clone, a comparison).
    #[test]
    fn a_layer_nested_at_any_depth_is_read_without_recursion() {
        let reader = std::thread::spawn(|| {
            for objects in [false, true] {
                for depth in [127, 128, 129, 1_000, 32_764, 100_000] {
                    let text = nested(depth, objects);
                    let read = object(&text).unwrap_or_else(|| panic!("{depth} {objects}"));
                    let read = Value::Object(read);
                    // What serde_json reads (127 deep) is its value; past
                    // that, 128 deep is kept, its last container empty.
                    match serde_json::from_str::<Value>(&text) {
                        Ok(theirs) => assert_eq!(read, theirs, "{depth} {objects}"),
                        Err(_) => assert!(depth >= 128, "serde_json refused {depth}"),
                    }
                    assert_eq!(depth_of(&read), depth.min(128), "{depth}");
                }
            }
        });
        reader.join().expect("read on the default stack");
    }

    /// Past the depth kept, the file is still read whole: a syntax error, or
    /// a key twice, 200 deep refuses it, as `std.json` refuses it.
    #[test]
    fn what_std_json_refuses_is_refused_at_any_depth() {
        let deep = |inside: &str, closes: usize| {
            format!(
                r#"{{"zzz": {}{inside}{}}}"#,
                "[".repeat(200),
                "]".repeat(closes)
            )
        };
        for (text, why) in [
            (deep(r#"{"a": 1, "a": 2}"#, 200), "a key twice, deep"),
            (
                deep(r#"{"a": 1, "\u0061": 2}"#, 200),
                "a key twice as decoded, deep",
            ),
            (deep("", 199), "torn, deep"),
            (deep("", 201), "a bracket too many"),
            (deep("1,", 200), "a trailing comma, deep"),
            (deep("01", 200), "a leading zero, deep"),
            (deep(r#""\ud800""#, 200), "a lone surrogate, deep"),
            (deep("\"\u{1}\"", 200), "a control byte, deep"),
            (deep("tru", 200), "a broken word, deep"),
            (deep("1e400 1", 200), "two values, deep"),
            (deep(r#"{"a" 1}"#, 200), "no colon, deep"),
        ] {
            assert_eq!(object(&text), None, "{why}");
        }
        for text in [
            "",
            " ",
            "{",
            "{}x",
            "{} {}",
            "\u{feff}{}",
            r#"{"a": 1,}"#,
            r#"{"a": [1,]}"#,
            r#"{,}"#,
            r#"{"a": .5}"#,
            r#"{"a": 1.}"#,
            r#"{"a": +1}"#,
            r#"{"a": 1e}"#,
            r#"{"a": -}"#,
            r#"{"a": 1-2}"#,
            r#"{"a": NaN}"#,
            r#"{"a": -Infinity}"#,
            r#"{"a": 'b'}"#,
            r#"{"a": "\x"}"#,
            r#"{"a": "b}"#,
            r#"{"a": "\"}"#,
            r#"{"a": 1, "a": 1}"#,
            r#"{a: 1}"#,
            "[1, 2]",
            "5",
        ] {
            assert_eq!(object(text), None, "{text:?}");
        }
        assert!(object(" \t\r\n{} \n").is_some());
    }

    /// A number `std.json` keeps as its text, `.number_string`, past `f64`
    /// (`dynamic.zig:30-47@0.17.0`): read, where serde_json refuses the file,
    /// as the largest `f64` of its sign -- a number that is no integer. One
    /// serde_json reads, past `i64` or below `f64`'s smallest, is serde_json's.
    #[test]
    fn a_number_past_f64_is_read_as_one_no_integer_key_takes() {
        let huge = "7".repeat(400);
        for (number, sign) in [
            ("1e400", 1.0),
            ("-1e400", -1.0),
            ("1.8e308", 1.0),
            ("1E+99999999999999999999", 1.0),
            (huge.as_str(), 1.0),
            (&format!("-{huge}"), -1.0),
        ] {
            let text = format!(r#"{{"zzz": {number}}}"#);
            assert!(serde_json::from_str::<Value>(&text).is_err(), "{number}");
            let read = object(&text).unwrap_or_else(|| panic!("{number}"));
            let read = &read["zzz"];
            assert_eq!(read.as_f64(), Some(sign * f64::MAX), "{number}");
            assert!(
                read.as_i64().is_none() && read.as_u64().is_none(),
                "{number}"
            );
        }
        for number in [
            "123456789012345678901234567890",
            "-123456789012345678901234567890",
            "9223372036854775808",
            "1e-400",
            "-0",
            "-0.0",
            "0e999999999999",
            "5.0",
            "7",
            "-7",
            "9223372036854775807",
        ] {
            let text = format!(r#"{{"zzz": {number}}}"#);
            let theirs: Value = serde_json::from_str(&text).expect("serde_json reads it");
            assert_eq!(
                object(&text).map(Value::Object),
                Some(theirs.clone()),
                "{number}"
            );
            // The same bits: `-0.0 == 0.0`, and a sign is part of the value.
            let bits = |value: &Value| value["zzz"].as_f64().map(f64::to_bits);
            assert_eq!(
                bits(&Value::Object(object(&text).unwrap())),
                bits(&theirs),
                "{number}"
            );
        }
    }

    /// A JSON value no object of which names a key twice, at any depth and
    /// as decoded: the rule this reader adds to serde_json's, said as a
    /// serde visitor of serde_json's own parse.
    struct KeysOnce;

    impl<'de> serde::Deserialize<'de> for KeysOnce {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(KeysOnce)
        }
    }

    impl<'de> serde::de::Visitor<'de> for KeysOnce {
        type Value = KeysOnce;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON value")
        }
        fn visit_bool<E>(self, _: bool) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_i64<E>(self, _: i64) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_u64<E>(self, _: u64) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_f64<E>(self, _: f64) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_str<E>(self, _: &str) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_unit<E>(self) -> Result<Self, E> {
            Ok(self)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut items: A) -> Result<Self, A::Error> {
            while items.next_element::<KeysOnce>()?.is_some() {}
            Ok(self)
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self, A::Error> {
            let mut keys = HashSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key) {
                    return Err(serde::de::Error::custom("a key twice"));
                }
                map.next_value::<KeysOnce>()?;
            }
            Ok(self)
        }
    }

    /// How one text read against serde_json's parse of its twin -- the same
    /// text with each number past `f64` at its sign's largest `f64` -- and
    /// [`KeysOnce`]: `None` when the twin is past serde_json's limits too.
    fn against_serde_json(text: &str, twin: &str) -> Option<&'static str> {
        let theirs = serde_json::from_str::<Value>(twin);
        if matches!(&theirs, Err(err) if err.to_string().starts_with("number out of range")) {
            return None;
        }
        let once = serde_json::from_str::<KeysOnce>(twin).is_ok();
        Some(match (value(text), theirs) {
            (Some(ours), Ok(theirs)) => {
                assert!(once, "{text:?}: a key twice was read");
                assert_eq!(ours, theirs, "{text:?}");
                if text == twin {
                    "same"
                } else {
                    "past f64"
                }
            }
            (None, Ok(_)) => {
                assert!(!once, "{text:?}: refused, and serde_json read it");
                "a key twice"
            }
            (Some(_), Err(err)) => panic!("{text:?}: read, and serde_json refused it: {err}"),
            (None, Err(_)) => "refused alike",
        })
    }

    /// A pseudo-random source, the same every run.
    fn source(mut seed: u64) -> impl FnMut() -> usize {
        move || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize
        }
    }

    /// The numbers past `f64`, and the largest `f64` of each one's sign.
    fn past_f64(token: &str) -> Option<&'static str> {
        match token {
            "1e400" => Some("1.7976931348623157e308"),
            "-1e400" => Some("-1.7976931348623157e308"),
            _ => None,
        }
    }

    /// Within serde_json's limits, this reader is serde_json's parse and a
    /// key twice refused: over 200,000 run-ons of JSON's pieces, and 50,000
    /// well-formed documents of a few keys (so a key twice is common), each is
    /// read to the same value or refused alike. A file serde_json refuses only
    /// for a number past `f64` is read as that file with the largest `f64` of
    /// the number's sign in its place.
    #[test]
    fn within_serde_jsons_limits_the_reader_is_serde_jsons_parse() {
        const PIECES: [&str; 40] = [
            "{",
            "}",
            "[",
            "]",
            ",",
            ":",
            " ",
            "\n",
            "\"a\"",
            "\"b\"",
            "\"\\u0061\"",
            "\"\\u00e9\"",
            "\"\\ud83d\\ude00\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"\\/\\t\"",
            "\"\u{1}\"",
            "\"\\\"\"",
            "\"é😀\"",
            "\"\\x\"",
            "0",
            "-0",
            "01",
            "1.5",
            "1.",
            "-",
            "1e5",
            "1E+2",
            "2e",
            "1e400",
            "-1e400",
            "1e-400",
            "123456789012345678901234567890",
            "true",
            "false",
            "null",
            "nul",
            "\t",
            "\u{feff}",
            "\r",
        ];
        let numeric = |byte: Option<u8>| {
            byte.is_some_and(|b| {
                b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E')
            })
        };
        let mut outcomes = std::collections::BTreeMap::<&str, usize>::new();
        let mut next = source(0x2026_1111);
        for _ in 0..200_000 {
            let pieces: Vec<&str> = (0..(next() % 14) + 1)
                .map(|_| PIECES[next() % PIECES.len()])
                .collect();
            // A number past `f64` run on into another number's bytes is
            // another token, not that number: such a text is left out.
            let run_on = pieces.iter().enumerate().any(|(at, piece)| {
                past_f64(piece).is_some()
                    && (numeric(
                        at.checked_sub(1)
                            .and_then(|before| pieces[before].bytes().last()),
                    ) || numeric(pieces.get(at + 1).and_then(|after| after.bytes().next())))
            });
            if run_on {
                continue;
            }
            let twin: String = pieces
                .iter()
                .map(|piece| past_f64(piece).unwrap_or(piece))
                .collect();
            if let Some(outcome) = against_serde_json(&pieces.concat(), &twin) {
                *outcomes.entry(outcome).or_default() += 1;
            }
        }
        // Documents: up to four deep, keys from three spellings of two.
        const KEYS: [&str; 3] = ["\"a\"", "\"b\"", "\"\\u0061\""];
        const SCALARS: [&str; 10] = [
            "0",
            "-0",
            "1e400",
            "-1e400",
            "1e-400",
            "123456789012345678901234567890",
            "\"x\"",
            "true",
            "null",
            "2.5",
        ];
        fn document(next: &mut dyn FnMut() -> usize, depth: usize, out: &mut (String, String)) {
            let push = |out: &mut (String, String), text: &str, twin: &str| {
                out.0.push_str(text);
                out.1.push_str(twin);
            };
            match next() % if depth == 0 { 1 } else { 4 } {
                0 => {
                    let scalar = SCALARS[next() % SCALARS.len()];
                    push(out, scalar, past_f64(scalar).unwrap_or(scalar));
                }
                1 => {
                    push(out, "[", "[");
                    for at in 0..next() % 4 {
                        push(
                            out,
                            if at == 0 { "" } else { ", " },
                            if at == 0 { "" } else { ", " },
                        );
                        document(next, depth - 1, out);
                    }
                    push(out, "]", "]");
                }
                _ => {
                    push(out, "{", "{");
                    for at in 0..next() % 4 {
                        let key = KEYS[next() % KEYS.len()];
                        let lead = if at == 0 { "" } else { ", " };
                        push(out, &format!("{lead}{key}: "), &format!("{lead}{key}: "));
                        document(next, depth - 1, out);
                    }
                    push(out, "}", "}");
                }
            }
        }
        let mut next = source(0x2026_1112);
        for _ in 0..50_000 {
            let mut texts = (String::new(), String::new());
            document(&mut next, 4, &mut texts);
            let outcome =
                against_serde_json(&texts.0, &texts.1).expect("within serde_json's limits");
            *outcomes.entry(outcome).or_default() += 1;
        }
        eprintln!("outcomes: {outcomes:?}");
        let at_least = [
            ("same", 10_000),
            ("past f64", 1_000),
            ("a key twice", 1_000),
        ];
        for (outcome, least) in at_least {
            assert!(
                outcomes.get(outcome).copied().unwrap_or(0) > least,
                "{outcomes:?}"
            );
        }
    }
}
