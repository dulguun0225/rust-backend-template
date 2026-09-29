//! A suppressed ban lint, in every form that lowers one.
#![warn(warnings)] // refused

#[cfg_attr(test, expect(clippy::unwrap_used, reason = "x"))] // refused
pub fn f(v: Option<u8>) -> u8 {
    v.unwrap_or(0)
}

#[warn(clippy::disallowed_methods)] // refused
pub fn lowered() {}

#[expect(renamed_and_removed_lints, reason = "a renamed ban name")] // refused
#[expect(clippy::disallowed_method, reason = "x")]
pub fn renamed() {}

macro_rules! quiet {
    ($($lint:tt)*) => {
        #[expect($($lint)*, reason = "x")] // refused
        pub fn hidden() {}
    };
}
quiet!(clippy::disallowed_methods);

#[warn(missing_docs)]
pub fn raised() {}
