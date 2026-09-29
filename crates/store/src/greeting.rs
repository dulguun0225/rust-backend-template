//! The `greeting` table: owned, and so written, by this module alone.

use db::versioned::{self, Outcome, OutcomeError};
use db::{DbError, Reads, WriteTx};
use std::num::NonZeroU16;

use platform::pager::{self, Cursor, Page};
use time::OffsetDateTime;
use uuid::Uuid;

/// A stored greeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreetingRow {
    /// The UUIDv7 key.
    pub id: Uuid,
    /// The name, 1 to 100 characters.
    pub name: String,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// The guarded-update counter.
    pub version: i64,
}

/// A guarded update's failure.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// The update's result contradicts the table's key.
    #[error(transparent)]
    Outcome(#[from] OutcomeError),
}

impl From<sqlx::Error> for UpdateError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(DbError::from(e))
    }
}

/// Inserts a new greeting at version 1.
///
/// # Errors
/// [`DbError`].
pub async fn insert(tx: &mut WriteTx, id: Uuid, name: &str, created_at: OffsetDateTime) -> Result<(), DbError> {
    sqlx::query!("insert into greeting (id, name, created_at, version) values ($1, $2, $3, 1)", id, name, created_at)
        .execute(tx.writer())
        .await?;
    Ok(())
}

/// The greeting with this id, if any.
///
/// # Errors
/// [`DbError`].
pub async fn find(tx: &mut impl Reads, id: Uuid) -> Result<Option<GreetingRow>, DbError> {
    let row = sqlx::query_as!(GreetingRow, "select id, name, created_at, version from greeting where id = $1", id)
        .fetch_optional(tx.conn())
        .await?;
    Ok(row)
}

/// Renames a greeting with the guarded update: one statement, the version increment and the two-predicate
/// guard, zero affected rows classified by a read in the same transaction.
///
/// # Errors
/// [`UpdateError`].
pub async fn rename(tx: &mut WriteTx, id: Uuid, expected_version: i64, name: &str) -> Result<Outcome, UpdateError> {
    let affected = sqlx::query!(
        "update greeting set name = $3, version = version + 1 where id = $1 and version = $2",
        id,
        expected_version,
        name
    )
    .execute(tx.writer())
    .await?
    .rows_affected();
    let current = if affected == 0 {
        sqlx::query_scalar!("select version from greeting where id = $1", id).fetch_optional(tx.writer()).await?
    } else {
        None
    };
    Ok(versioned::outcome(affected, expected_version, current)?)
}

/// One page of greetings, newest first, after `cursor`: the keyset pager's query shape.
///
/// Two statements rather than one with `$1 is null or …`: under a generic plan, which PostgreSQL may choose for a
/// prepared statement after five runs, that predicate is a filter rather than an index condition, and a deep page
/// reads every row before it, as `OFFSET` would (run 2026-09-29: 199,990 rows removed by the filter for a page near
/// the end of 200,000; 4 buffers for the same page with the condition on the index).
///
/// # Errors
/// [`DbError`].
pub async fn page(
    tx: &mut impl Reads,
    cursor: Option<Cursor>,
    limit: NonZeroU16,
) -> Result<Page<GreetingRow>, DbError> {
    let fetch = i64::from(limit.get()).checked_add(1);
    let fetched = match cursor {
        None => {
            sqlx::query_as!(
                GreetingRow,
                "select id, name, created_at, version from greeting
                 order by created_at desc, id desc
                 limit $1",
                fetch
            )
            .fetch_all(tx.conn())
            .await?
        }
        Some(after) => {
            sqlx::query_as!(
                GreetingRow,
                "select id, name, created_at, version from greeting
                 where (created_at, id) < ($1, $2)
                 order by created_at desc, id desc
                 limit $3",
                after.sort,
                after.id,
                fetch
            )
            .fetch_all(tx.conn())
            .await?
        }
    };
    Ok(pager::to_page(fetched, limit, |row| Cursor { sort: row.created_at, id: row.id }))
}
