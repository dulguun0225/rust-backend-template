//! What a schema declares for one scalar input, read the same way for a body member and for a path variable,
//! query parameter or header: the word `expected` carries, the JSON type, and the strict forms of text.

use serde_json::{Number, Value};

/// Every word `expected` may carry: the formats a schema here declares, then the JSON types. A word outside
/// it is never sent, so `expected` is never built from anything but this list.
const EXPECTED: [&str; 13] = [
    "uuid",
    "date-time",
    "date",
    "int32",
    "int64",
    "float",
    "double",
    "integer",
    "number",
    "boolean",
    "string",
    "object",
    "array",
];

/// The schema's `format`, else its `type`, as a word of the fixed vocabulary; `string` when it names neither.
pub(crate) fn expected(schema: &Value) -> &'static str {
    let word = |key: &str| declared_word(schema.get(key)).and_then(|w| EXPECTED.into_iter().find(|e| *e == w));
    word("format").or_else(|| word("type")).unwrap_or("string")
}

/// The JSON type a schema declares, null aside.
pub(crate) fn json_type(schema: &Value) -> Option<&str> {
    declared_word(schema.get("type"))
}

/// A `type` or `format` value: a string, or the first entry of an array that is not `null`.
fn declared_word(value: Option<&Value>) -> Option<&str> {
    let value = value?;
    value.as_str().or_else(|| value.as_array()?.iter().filter_map(Value::as_str).find(|w| *w != "null"))
}

/// RFC 9562's one string form of a UUID: 36 characters, hyphens at their four places, hexadecimal digits of
/// either case. The `uuid` crate also parses the 32-digit, braced and `urn:uuid:` forms, none of them 36
/// characters long. Never trimmed.
pub(crate) fn is_uuid_text(text: &str) -> bool {
    text.len() == 36 && uuid::Uuid::try_parse(text).is_ok()
}

/// The text as a JSON number, when it is exactly one: no surrounding whitespace, no sign `+`, no leading zero.
pub(crate) fn number(text: &str) -> Option<Number> {
    let untrimmed = text.trim_matches([' ', '\t', '\n', '\r']).len() == text.len();
    untrimmed.then(|| serde_json::from_slice(text.as_bytes()).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn expected_is_the_format_else_the_type_from_a_fixed_vocabulary() {
        assert_eq!(super::expected(&json!({ "type": "string", "format": "uuid" })), "uuid");
        assert_eq!(super::expected(&json!({ "type": ["integer", "null"], "format": "int32" })), "int32");
        assert_eq!(super::expected(&json!({ "type": ["null", "integer"] })), "integer");
        assert_eq!(super::expected(&json!({ "type": "string", "format": "email" })), "string");
        assert_eq!(super::expected(&json!({ "type": "boolean" })), "boolean");
        assert_eq!(super::expected(&json!({ "type": "whatever" })), "string");
        assert_eq!(super::expected(&json!({ "$ref": "#/components/schemas/Kind" })), "string");
    }

    #[test]
    fn the_json_type_skips_null() {
        assert_eq!(super::json_type(&json!({ "type": ["null", "number"] })), Some("number"));
        assert_eq!(super::json_type(&json!({ "type": "boolean" })), Some("boolean"));
        assert_eq!(super::json_type(&json!({ "type": 7 })), None);
        assert_eq!(super::json_type(&json!({})), None);
    }

    #[test]
    fn a_uuid_is_its_36_character_form_only() {
        assert!(super::is_uuid_text("0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2"));
        assert!(super::is_uuid_text("0192F0C1-8B39-7CC4-9A41-6F8E62A4A1B2"));
        for refused in [
            "0192f0c18b397cc49a416f8e62a4a1b2",
            "{0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2}",
            "urn:uuid:0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2",
            " 0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2",
            "0192f0c1-8b39-7cc4-9a41-6f8e62a4a1bz",
            "",
        ] {
            assert!(!super::is_uuid_text(refused), "{refused}");
        }
    }

    #[test]
    fn a_number_is_exactly_one_json_number() {
        assert_eq!(super::number("-12"), Some((-12).into()));
        assert!(super::number("0.5").is_some_and(|n| n.is_f64()));
        for refused in ["", " 1", "1 ", "\t1", "1\n", "+1", "01", "1.", "abc", "\"1\"", "NaN"] {
            assert_eq!(super::number(refused), None, "{refused:?}");
        }
    }
}
