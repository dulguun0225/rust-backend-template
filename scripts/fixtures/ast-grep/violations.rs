//! One violation per serde rule, for the wall's exit-code canary: `ast-grep scan` over this file must exit 1.

#[derive(serde::Deserialize)]
pub struct Open {
    pub name: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Optional {
    pub nickname: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaulted {
    #[serde(default)]
    pub count: u8,
}

// A renamed Deserialize derive.
use serde::Deserialize as Lenient;
