//! Source no rule here reads.

#[path = "hidden.txt"] // refused
mod hidden;

#[cfg_attr(test, path = "other.rs")] // refused
mod other;

include!("generated.inc"); // refused

pub fn text() -> &'static str {
    include_str!("lib.rs")
}
