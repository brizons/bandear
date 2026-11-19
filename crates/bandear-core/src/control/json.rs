//! JSON read and written as the iOS client's Foundation coders read and write it.
//!
//! Reading parses with `serde_json` into a [`Value`], then reads each field with the conversions
//! below, which are `JSONDecoder`'s: a whole number is any number that is exactly one (`2.0` and
//! `1e2` are, `2.5` is not), a `true` is never a number and a number never a `true`.
//!
//! Writing is done by hand, because `JSONEncoder`'s bytes are not `serde_json`'s: it writes a
//! whole `Double` without its `.0` (`40`, not `40.0`) and, unless told otherwise, a `/` as `\/`.
//! What the phone sends stays byte for byte what the iOS client sent.
//!
//! Three things `serde_json` reads differently from `JSONDecoder`, none of which the desk writes
//! (it writes with `serde_json` itself): a document nested more than 128 levels deep, or holding a
//! number beyond a `Double`'s range (`1e400`), is not JSON at all here, where `JSONDecoder` only
//! refuses such a value when a field it reads holds it (and nests up to 512 levels); and of two
//! fields with the same key the last one counts here, the first one there.

use serde_json::Value;

/// 2⁶³, the first whole number past `i64::MAX`, exactly.
const TWO_TO_THE_63: f64 = (1u64 << 63) as f64;
/// 2⁶⁴, the first whole number past `u64::MAX`, exactly.
const TWO_TO_THE_64: f64 = 2.0 * TWO_TO_THE_63;

/// `text` parsed, or `None` when it is not JSON.
pub(super) fn parse(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// `value` as `JSONDecoder` reads an `Int`: an integer in range, or a number written with a
/// fraction or an exponent that is exactly such an integer. `None` for anything else.
pub(super) fn integer(value: &Value) -> Option<i64> {
    let Value::Number(number) = value else {
        return None;
    };
    if let Some(integer) = number.as_i64() {
        return Some(integer);
    }
    if number.is_u64() {
        // Past `i64::MAX`.
        return None;
    }
    let float = number.as_f64()?;
    (float.fract() == 0.0 && (-TWO_TO_THE_63..TWO_TO_THE_63).contains(&float))
        .then_some(float as i64)
}

/// `value` as `JSONDecoder` reads a `UInt64`: like [`integer`], from 0 (`-0` included) to
/// `u64::MAX`.
pub(super) fn unsigned(value: &Value) -> Option<u64> {
    let Value::Number(number) = value else {
        return None;
    };
    if let Some(unsigned) = number.as_u64() {
        return Some(unsigned);
    }
    if number.is_i64() {
        // Below zero.
        return None;
    }
    let float = number.as_f64()?;
    // `-0.0` is in range, as `JSONDecoder` reads `-0` as 0.
    (float.fract() == 0.0 && (0.0..TWO_TO_THE_64).contains(&float)).then_some(float as u64)
}

/// `value` as `JSONDecoder` reads a `UInt8`.
pub(super) fn byte(value: &Value) -> Option<u8> {
    unsigned(value).and_then(|unsigned| u8::try_from(unsigned).ok())
}

/// `value` as `JSONDecoder` reads a `UInt32`.
pub(super) fn unsigned_32(value: &Value) -> Option<u32> {
    unsigned(value).and_then(|unsigned| u32::try_from(unsigned).ok())
}

/// `value` as `JSONDecoder` reads a `Double`: any number, a whole one included; never a `true`.
pub(super) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

/// `value` as `JSONDecoder` reads a `[Int]`: a list whose every entry is an [`integer`].
pub(super) fn integers(value: &Value) -> Option<Vec<i64>> {
    value.as_array()?.iter().map(integer).collect()
}

/// `value` as `JSONDecoder` reads a `[Bool]`: a list whose every entry is `true` or `false`.
pub(super) fn booleans(value: &Value) -> Option<Vec<bool>> {
    value.as_array()?.iter().map(Value::as_bool).collect()
}

/// Whether a `/` in a string is written `\/`: `JSONEncoder`'s default, which the control messages
/// are written with, or as itself, as the saved mixes are written (`.withoutEscapingSlashes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slashes {
    Escaped,
    Plain,
}

/// A JSON object written field by field, in the order the fields are added, as `JSONEncoder`
/// writes them: no spaces, strings escaped as [`write_string`] escapes them.
pub(super) struct ObjectWriter {
    text: String,
    slashes: Slashes,
    is_empty: bool,
}

impl ObjectWriter {
    pub(super) fn new(slashes: Slashes) -> Self {
        Self {
            text: String::from("{"),
            slashes,
            is_empty: true,
        }
    }

    pub(super) fn string(&mut self, key: &str, value: &str) {
        self.key(key);
        write_string(&mut self.text, value, self.slashes);
    }

    pub(super) fn unsigned(&mut self, key: &str, value: u64) {
        self.key(key);
        self.text.push_str(&value.to_string());
    }

    pub(super) fn integer(&mut self, key: &str, value: i64) {
        self.key(key);
        self.text.push_str(&value.to_string());
    }

    pub(super) fn boolean(&mut self, key: &str, value: bool) {
        self.key(key);
        self.text.push_str(if value { "true" } else { "false" });
    }

    /// `value` as `JSONEncoder` writes a `Double`: its shortest text that reads back as the same
    /// number, without a trailing `.0` (`40`, `1.25`, `-0`). Both write a number from 0 to 60 000
    /// in plain digits; outside that, `JSONEncoder` switches to an exponent where this does not,
    /// and no caller writes such a number. JSON has no non-finite number, which `JSONEncoder`
    /// refuses to write; this writes `null` for one, and no caller passes one.
    pub(super) fn number(&mut self, key: &str, value: f64) {
        self.key(key);
        if value.is_finite() {
            self.text.push_str(&value.to_string());
        } else {
            self.text.push_str("null");
        }
    }

    pub(super) fn integers(&mut self, key: &str, values: &[i64]) {
        self.key(key);
        self.list(values.iter().map(i64::to_string));
    }

    pub(super) fn booleans(&mut self, key: &str, values: &[bool]) {
        self.key(key);
        self.list(values.iter().map(bool::to_string));
    }

    pub(super) fn strings<'a>(&mut self, key: &str, values: impl IntoIterator<Item = &'a str>) {
        self.key(key);
        let slashes = self.slashes;
        self.list(values.into_iter().map(|value| {
            let mut text = String::new();
            write_string(&mut text, value, slashes);
            text
        }));
    }

    /// The object's text.
    pub(super) fn finish(mut self) -> String {
        self.text.push('}');
        self.text
    }

    fn key(&mut self, key: &str) {
        if !self.is_empty {
            self.text.push(',');
        }
        self.is_empty = false;
        write_string(&mut self.text, key, self.slashes);
        self.text.push(':');
    }

    fn list(&mut self, entries: impl Iterator<Item = String>) {
        self.text.push('[');
        for (index, entry) in entries.enumerate() {
            if index > 0 {
                self.text.push(',');
            }
            self.text.push_str(&entry);
        }
        self.text.push(']');
    }
}

/// `entries`, each already JSON, as one JSON list.
pub(super) fn list(entries: impl IntoIterator<Item = String>) -> String {
    let mut text = String::from("[");
    for (index, entry) in entries.into_iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&entry);
    }
    text.push(']');
    text
}

/// `value` as a JSON string, escaped as `JSONEncoder` escapes one: `"` and `\` with a backslash,
/// `/` as `\/` unless [`Slashes::Plain`], the five control characters JSON names (`\b`, `\f`, `\n`,
/// `\r`, `\t`) by name and the other ones below U+0020 as `\u00xx` in lowercase hex. Everything else,
/// U+007F and U+2028 included, is written as itself.
pub(super) fn write_string(text: &mut String, value: &str, slashes: Slashes) {
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '/' if slashes == Slashes::Escaped => text.push_str("\\/"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                text.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => text.push(other),
        }
    }
    text.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Not ported: what `JSONEncoder` and `JSONDecoder` do, read off them on the Mac the iOS client
    // is built on, so the bytes and the leniency stay the iOS client's.

    #[test]
    fn strings_are_escaped_as_json_encoder_escapes_them() {
        let mut escaped = String::new();
        write_string(
            &mut escaped,
            "a/b\u{1}\u{1f}\"\\\u{7f}\u{2028}é\n\t\u{8}\u{c}\r",
            Slashes::Escaped,
        );
        assert_eq!(
            escaped,
            "\"a\\/b\\u0001\\u001f\\\"\\\\\u{7f}\u{2028}é\\n\\t\\b\\f\\r\""
        );

        let mut plain = String::new();
        write_string(&mut plain, "AC/DC", Slashes::Plain);
        assert_eq!(plain, "\"AC/DC\"");
    }

    #[test]
    fn numbers_are_written_as_json_encoder_writes_them() {
        let mut object = ObjectWriter::new(Slashes::Escaped);
        for (key, value) in [
            ("a", 40.0),
            ("b", 1.25),
            ("c", -0.0),
            ("d", 0.01),
            ("e", 59_999.99),
        ] {
            object.number(key, value);
        }
        object.number("f", f64::NAN);
        object.unsigned("g", u64::MAX);
        object.integer("h", i64::MIN);
        assert_eq!(
            object.finish(),
            r#"{"a":40,"b":1.25,"c":-0,"d":0.01,"e":59999.99,"f":null,"g":18446744073709551615,"h":-9223372036854775808}"#
        );
    }

    #[test]
    fn whole_numbers_are_read_as_json_decoder_reads_them() {
        let read = |text: &str| integer(&parse(text).expect("JSON"));
        assert_eq!(read("2.0"), Some(2));
        assert_eq!(read("1e2"), Some(100));
        assert_eq!(read("1E2"), Some(100));
        assert_eq!(read("-0"), Some(0));
        assert_eq!(read("-0.0"), Some(0));
        assert_eq!(read("9223372036854775807"), Some(i64::MAX));
        assert_eq!(read("9223372036854775808"), None);
        assert_eq!(read("2.5"), None);
        assert_eq!(read("\"3\""), None);
        assert_eq!(read("true"), None);

        let read = |text: &str| unsigned(&parse(text).expect("JSON"));
        assert_eq!(read("2.0"), Some(2));
        assert_eq!(read("-0"), Some(0));
        assert_eq!(read("-1"), None);
        assert_eq!(read("18446744073709551615"), Some(u64::MAX));
        assert_eq!(read("18446744073709551616"), None);

        assert_eq!(byte(&json!(255)), Some(255));
        assert_eq!(byte(&json!(256)), None);
        assert_eq!(number(&json!(1)), Some(1.0));
        assert_eq!(number(&json!(true)), None);
        assert_eq!(booleans(&json!([true, 1])), None);
        assert_eq!(booleans(&json!([false, null])), None);
    }

    /// The differences the module's documentation names, pinned.
    #[test]
    fn what_serde_json_reads_differently_from_json_decoder() {
        assert_eq!(parse(r#"{"x":1e400}"#), None);
        let deep = format!(r#"{{"x":{}{}}}"#, "[".repeat(200), "]".repeat(200));
        assert_eq!(parse(&deep), None);
        assert_eq!(parse(r#"{"v":1,"v":2}"#), Some(json!({"v": 2})));
    }
}
