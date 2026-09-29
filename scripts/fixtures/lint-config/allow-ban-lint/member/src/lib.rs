//! A suppressed ban lint.

#[cfg_attr(test, expect(clippy::unwrap_used, reason = "x"))]
pub fn f(v: Option<u8>) -> u8 {
    v.unwrap_or(0)
}
