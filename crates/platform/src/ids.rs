//! The one id producer: UUIDv7 (48-bit millisecond timestamp, version 7, RFC 9562 variant). A random key
//! scatters the primary-key index; v7 keeps inserts at its right edge. `clippy.toml` bans `Uuid::now_v7` and
//! `Uuid::new_v7` outside this crate; `uuid`'s `v4` feature is not enabled, and `deny.toml` refuses it.

use uuid::Uuid;

/// A fresh UUIDv7. Ids made in one process are strictly increasing.
#[must_use]
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

#[cfg(test)]
mod tests {
    use super::new_id;

    #[test]
    fn an_id_is_version_7_with_the_rfc_variant() {
        let id = new_id();
        assert_eq!(id.get_version_num(), 7);
        assert_eq!(id.get_variant(), uuid::Variant::RFC4122);
    }

    #[test]
    fn ids_made_in_sequence_increase() {
        let ids: Vec<_> = (0..1_000).map(|_| new_id()).collect();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
