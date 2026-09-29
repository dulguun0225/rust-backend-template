//! The keyset pager: no offset, no totals. Pages run descending over a `(timestamp, id)` key; the id is the
//! final tiebreak after a business sort key and never the sole sort (`scripts/check-sql.mjs` refuses an
//! `ORDER BY` that starts with an id column and any `OFFSET`). The cursor is an opaque base64url token of the
//! last row's exact position, so a malformed cursor fails loud rather than mis-seeking.
//!
//! A query pages with `WHERE (created_at, id) < ($1, $2) ORDER BY created_at DESC, id DESC LIMIT $3`,
//! fetching `limit + 1` rows, and hands them to [`to_page`]; the first page is a second statement without the
//! `WHERE`, since `$1 IS NULL OR …` is a filter under a generic plan and a deep page then reads every row before it.

use std::num::NonZeroU16;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use time::OffsetDateTime;
use uuid::Uuid;

/// The largest page a client may ask for; the document declares it as the `limit` parameter's maximum.
pub const MAX_LIMIT: u16 = 100;

const VERSION: u8 = 1;
const ENCODED_LEN: usize = 33;

/// A decoded seek position: the last row's sort value and its id tiebreak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// The last row's sort value.
    pub sort: OffsetDateTime,
    /// The last row's id.
    pub id: Uuid,
}

/// One page: at most `limit` items, and the cursor to the next page, `None` at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The rows of this page.
    pub items: Vec<T>,
    /// The cursor to the next page, or `None` when this page is the last.
    pub next_cursor: Option<String>,
}

/// A cursor that did not come from [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the cursor is not one this service issued")]
pub struct InvalidCursor;

/// The opaque token for a position.
#[must_use]
pub fn encode(cursor: Cursor) -> String {
    let mut bytes = Vec::with_capacity(ENCODED_LEN);
    bytes.push(VERSION);
    bytes.extend_from_slice(&cursor.sort.unix_timestamp_nanos().to_be_bytes());
    bytes.extend_from_slice(cursor.id.as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes a client's cursor: `None` for the first page (absent or empty), an error for anything else that
/// [`encode`] did not produce.
///
/// # Errors
/// [`InvalidCursor`] when the token is not base64url, has the wrong length or version, or holds an instant
/// out of range.
pub fn decode(token: Option<&str>) -> Result<Option<Cursor>, InvalidCursor> {
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD.decode(token).map_err(|_| InvalidCursor)?;
    let (version, rest) = bytes.split_first().ok_or(InvalidCursor)?;
    if *version != VERSION || rest.len() != ENCODED_LEN.saturating_sub(1) {
        return Err(InvalidCursor);
    }
    let (nanos, id) = rest.split_at_checked(16).ok_or(InvalidCursor)?;
    let nanos = i128::from_be_bytes(nanos.try_into().map_err(|_| InvalidCursor)?);
    let sort = OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| InvalidCursor)?;
    let id = Uuid::from_slice(id).map_err(|_| InvalidCursor)?;
    Ok(Some(Cursor { sort, id }))
}

/// Assembles a page from the `limit + 1` rows a query fetched: an extra row means a next page exists, and the
/// last row kept supplies its cursor. The limit is at least one: a page that kept no row would have no row to
/// point past, and would end the list while rows remain.
#[must_use]
pub fn to_page<T>(mut fetched: Vec<T>, limit: NonZeroU16, position: impl Fn(&T) -> Cursor) -> Page<T> {
    let limit = usize::from(limit.get());
    if fetched.len() <= limit {
        return Page { items: fetched, next_cursor: None };
    }
    fetched.truncate(limit);
    let next_cursor = fetched.last().map(|row| encode(position(row)));
    Page { items: fetched, next_cursor }
}

#[cfg(test)]
mod tests {
    use super::{Cursor, InvalidCursor, decode, encode, to_page};
    use base64::Engine as _;
    use proptest::prelude::*;
    use std::num::NonZeroU16;
    use time::macros::datetime;
    use uuid::Uuid;

    fn token_bytes(token: &str) -> Vec<u8> {
        super::URL_SAFE_NO_PAD.decode(token).unwrap_or_default()
    }

    fn reencode(bytes: &[u8]) -> String {
        super::URL_SAFE_NO_PAD.encode(bytes)
    }

    fn limit(n: u16) -> NonZeroU16 {
        NonZeroU16::new(n).unwrap()
    }

    fn cursor(n: u128) -> Cursor {
        Cursor { sort: datetime!(2026-09-29 10:00:00.000001 UTC), id: Uuid::from_u128(n) }
    }

    #[test]
    fn a_cursor_round_trips() {
        let c = cursor(7);
        assert_eq!(decode(Some(&encode(c))), Ok(Some(c)));
    }

    #[test]
    fn no_cursor_is_the_first_page() {
        assert_eq!(decode(None), Ok(None));
        assert_eq!(decode(Some("")), Ok(None));
    }

    #[test]
    fn a_malformed_cursor_fails_loud() {
        let good = encode(cursor(1));
        for bad in ["%%%", "AAAA", &good[..good.len() - 2], &format!("{good}AA")] {
            assert_eq!(decode(Some(bad)), Err(InvalidCursor), "{bad}");
        }
        let mut wrong_version = token_bytes(&good);
        wrong_version[0] = 2;
        assert_eq!(decode(Some(&reencode(&wrong_version))), Err(InvalidCursor));
    }

    #[test]
    fn a_page_drops_the_extra_row_and_points_past_the_last_kept() {
        let rows: Vec<u128> = (1..=4).collect();
        let page = to_page(rows, limit(3), |n| cursor(*n));
        assert_eq!(page.items, [1, 2, 3]);
        assert_eq!(decode(page.next_cursor.as_deref()), Ok(Some(cursor(3))));
    }

    #[test]
    fn a_page_of_one_points_past_its_row() {
        let page = to_page(vec![1_u128, 2], limit(1), |n| cursor(*n));
        assert_eq!(page.items, [1]);
        assert_eq!(decode(page.next_cursor.as_deref()), Ok(Some(cursor(1))));
    }

    #[test]
    fn the_last_page_has_no_cursor() {
        let page = to_page(vec![1_u128, 2], limit(3), |n| cursor(*n));
        assert_eq!(page.items, [1, 2]);
        assert_eq!(page.next_cursor, None);
    }

    proptest! {
        #[test]
        fn every_position_round_trips(nanos in -377_705_116_800_000_000_000_i128..253_402_300_799_999_999_999_i128, id: u128) {
            let sort = time::OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap();
            let c = Cursor { sort, id: Uuid::from_u128(id) };
            prop_assert_eq!(decode(Some(&encode(c))), Ok(Some(c)));
        }
    }
}
