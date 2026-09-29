//! Code the release build or the coverage build compiles and clippy, on the dev profile with every feature,
//! never reads.

#[cfg(not(debug_assertions))] // refused
pub fn release_only() {}

#[cfg_attr(debug_assertions, doc = "x")] // refused
pub fn attr() {}

#[cfg(not(coverage))] // refused
pub fn unmeasured() {}

#[cfg(not(feature = "lint"))] // refused
pub fn default_features_only() {}

#[cfg(all(unix, not(any(test, feature = "lint"))))] // refused
pub fn nested() {}

#[cfg(feature = "lint")]
pub fn every_feature() {}

#[cfg(not(test))]
pub fn not_test() {}

pub fn expression() -> bool {
    cfg!(debug_assertions)
}
