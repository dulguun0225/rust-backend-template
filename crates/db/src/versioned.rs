//! The guarded version-column update (`java-backend-api` *The guarded version-column update*, carried here
//! as its Rust form). An `UPDATE` on a table with a `version` column is one statement,
//!
//! ```sql
//! update <table> set <changes>, version = version + 1 where id = $1 and version = $2
//! ```
//!
//! and `scripts/check-sql.mjs` refuses any other spelling. Zero affected rows is a signal, never a no-op: the
//! caller reads the row's current version in the same transaction and [`outcome`] classifies the result.
//! The expected version must be one read in the same transaction; a version read earlier proves nothing.

/// What the guarded update did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The row moved from the expected version to `new_version`.
    Applied {
        /// The version after the update.
        new_version: i64,
    },
    /// The row exists at `current_version`; the caller's precondition is stale.
    Stale {
        /// The version the row is at.
        current_version: i64,
    },
    /// No row has that id.
    Absent,
}

/// Why a guarded update's result cannot be classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OutcomeError {
    /// More than one row matched a primary key.
    #[error("a guarded update affected {0} rows; the id is a primary key")]
    SeveralRows(u64),
    /// The expected version is at its maximum.
    #[error("the version counter is exhausted")]
    Exhausted,
    /// No row was updated, yet the row is at the expected version.
    #[error("no row updated, yet the row is at the expected version")]
    Inconsistent,
}

/// Classifies a guarded update: `affected` from the `UPDATE`, and when it is zero, `current` from a
/// `select version … where id = $1` in the same transaction.
///
/// # Errors
/// [`OutcomeError`] when the numbers contradict a primary key or the counter.
pub fn outcome(affected: u64, expected: i64, current: Option<i64>) -> Result<Outcome, OutcomeError> {
    match (affected, current) {
        (1, _) => {
            expected.checked_add(1).map(|new_version| Outcome::Applied { new_version }).ok_or(OutcomeError::Exhausted)
        }
        (0, None) => Ok(Outcome::Absent),
        (0, Some(current_version)) if current_version == expected => Err(OutcomeError::Inconsistent),
        (0, Some(current_version)) => Ok(Outcome::Stale { current_version }),
        (n, _) => Err(OutcomeError::SeveralRows(n)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, OutcomeError, outcome};

    #[test]
    fn each_affected_count_is_classified() {
        assert_eq!(outcome(1, 3, None), Ok(Outcome::Applied { new_version: 4 }));
        assert_eq!(outcome(0, 3, None), Ok(Outcome::Absent));
        assert_eq!(outcome(0, 3, Some(5)), Ok(Outcome::Stale { current_version: 5 }));
        assert_eq!(outcome(0, 3, Some(3)), Err(OutcomeError::Inconsistent));
        assert_eq!(outcome(2, 3, None), Err(OutcomeError::SeveralRows(2)));
        assert_eq!(outcome(1, i64::MAX, None), Err(OutcomeError::Exhausted));
    }
}
