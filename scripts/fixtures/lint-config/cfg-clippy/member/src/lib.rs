//! Code clippy never reads.

#[cfg(not(clippy))]
pub fn hidden(v: Option<u8>) -> u8 {
    v.unwrap()
}

#[cfg(clippy)]
pub fn hidden(_v: Option<u8>) -> u8 {
    0
}
