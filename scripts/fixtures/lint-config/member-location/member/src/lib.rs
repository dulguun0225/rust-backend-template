//! A member that breaks nothing.

#[expect(dead_code, reason = "a fixture")]
fn unused() {}

/// `#[allow(clippy::unwrap_used)]` in a doc comment is not an attribute.
pub fn text() -> &'static str {
    "#[cfg(clippy)] in a string is not an attribute"
}
