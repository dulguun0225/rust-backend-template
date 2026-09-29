//! Time comes from an injected [`Clock`]. `clippy.toml` bans every wall-clock read outside this file's crate,
//! and nothing else in this crate reads one.

use time::OffsetDateTime;

/// The source of the current instant. Services take `Arc<dyn Clock>`; tests pass a [`FixedClock`].
pub trait Clock: Send + Sync + core::fmt::Debug {
    /// The current instant, in UTC, truncated to microseconds: PostgreSQL's `timestamptz` keeps
    /// microseconds, so a value read back equals the value written.
    fn now(&self) -> OffsetDateTime;
}

/// The production clock: the operating system's wall clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        truncate_to_micros(OffsetDateTime::now_utc())
    }
}

/// A clock that always answers the same instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(OffsetDateTime);

impl FixedClock {
    /// A clock stopped at `at`, truncated to microseconds as [`Clock::now`] promises.
    #[must_use]
    pub fn at(at: OffsetDateTime) -> Self {
        Self(truncate_to_micros(at))
    }
}

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}

fn truncate_to_micros(at: OffsetDateTime) -> OffsetDateTime {
    at.replace_microsecond(at.microsecond()).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock, SystemClock};
    use time::macros::datetime;

    #[test]
    fn a_fixed_clock_answers_its_instant_in_microseconds() {
        let clock = FixedClock::at(datetime!(2026-09-29 10:00:00.123456789 UTC));
        assert_eq!(clock.now(), datetime!(2026-09-29 10:00:00.123456 UTC));
    }

    #[test]
    fn the_system_clock_answers_whole_microseconds_in_utc() {
        let now = SystemClock.now();
        assert_eq!(now.nanosecond().checked_rem(1_000), Some(0));
        assert!(now.offset().is_utc());
    }
}
