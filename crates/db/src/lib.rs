//! The database platform tier.
//!
//! - [`MIGRATOR`]: the committed migrations under `migrations/`, applied at startup and by `#[sqlx::test]`.
//! - [`Tx`]: the one transaction seam. `clippy.toml` bans `Pool::begin`, `Pool::acquire` and their kin in
//!   every other crate, so every statement runs inside a visible `tx.read(…)` or `tx.write(…)` block.
//! - [`versioned`]: what a guarded version-column update did.

pub mod versioned;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// The committed migrations, embedded at build time.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// A database failure below any feature: the edge turns it into a coded 500.
#[derive(Debug, thiserror::Error)]
#[error("database: {0}")]
pub struct DbError(#[from] sqlx::Error);

/// The one transaction seam. Cheap to clone; every clone shares the pool.
#[derive(Debug, Clone)]
pub struct Tx {
    pool: PgPool,
    begun: Arc<AtomicU64>,
}

/// A read-only transaction: PostgreSQL refuses a write inside it.
#[derive(Debug)]
pub struct ReadTx(Transaction<'static, Postgres>);

/// A read-write transaction.
#[derive(Debug)]
pub struct WriteTx(Transaction<'static, Postgres>);

/// A transaction a query may read through: both [`ReadTx`] and [`WriteTx`].
pub trait Reads {
    /// The connection a checked query runs on.
    fn conn(&mut self) -> &mut PgConnection;
}

impl Reads for ReadTx {
    fn conn(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

impl Reads for WriteTx {
    fn conn(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

impl WriteTx {
    /// The connection a checked write runs on. Only a [`WriteTx`] has one, so a store function that writes
    /// takes `&mut WriteTx` and cannot be called inside [`Tx::read`].
    pub fn writer(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

/// Connects a pool of at most `max_connections`, applies [`MIGRATOR`], and returns the seam over it.
///
/// # Errors
/// [`DbError`] when the database is unreachable or a migration fails, including an applied migration whose
/// file changed since.
pub async fn open(url: &str, max_connections: u32) -> Result<Tx, DbError> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;
    MIGRATOR.run(&pool).await.map_err(sqlx::Error::from)?;
    Ok(Tx::new(pool))
}

impl Tx {
    /// The seam over a pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool, begun: Arc::new(AtomicU64::new(0)) }
    }

    /// Runs `work` in a read-only transaction; commits on `Ok`, rolls back on `Err`.
    ///
    /// # Errors
    /// `work`'s error, or [`DbError`] from beginning or committing.
    pub async fn read<T, E>(&self, work: impl AsyncFnOnce(&mut ReadTx) -> Result<T, E>) -> Result<T, E>
    where
        E: From<DbError>,
    {
        let mut tx = ReadTx(self.begin("BEGIN ISOLATION LEVEL READ COMMITTED, READ ONLY").await?);
        let value = work(&mut tx).await?;
        tx.0.commit().await.map_err(DbError::from)?;
        Ok(value)
    }

    /// Runs `work` in a read-write transaction; commits on `Ok`, rolls back on `Err`.
    ///
    /// # Errors
    /// `work`'s error, or [`DbError`] from beginning or committing.
    pub async fn write<T, E>(&self, work: impl AsyncFnOnce(&mut WriteTx) -> Result<T, E>) -> Result<T, E>
    where
        E: From<DbError>,
    {
        let mut tx = WriteTx(self.begin("BEGIN ISOLATION LEVEL READ COMMITTED, READ WRITE").await?);
        let value = work(&mut tx).await?;
        tx.0.commit().await.map_err(DbError::from)?;
        Ok(value)
    }

    /// How many transactions this seam has begun: a test measures that a refused request opened none.
    #[must_use]
    pub fn begun(&self) -> u64 {
        self.begun.load(Ordering::Relaxed)
    }

    async fn begin(&self, statement: &'static str) -> Result<Transaction<'static, Postgres>, DbError> {
        let tx = self.pool.begin_with(statement).await?;
        self.begun.fetch_add(1, Ordering::Relaxed);
        Ok(tx)
    }
}
