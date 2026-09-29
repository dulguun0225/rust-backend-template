//! The NUMERIC mapper against a real PostgreSQL: values cross the wire protocol through a `numeric(19,4)`
//! cast, exactly as a `NUMERIC(19,4)` column hands them over. `sqlx.toml` in this crate makes every NUMERIC
//! decode and encode as `DbAmount`.

#![cfg(test)]

use db::{DbError, Reads as _, Tx};
use money::{Currency, Money};
use money_sql::{DbAmount, MapError};
use sqlx::PgPool;

fn currency(code: &str) -> Currency {
    Currency::from_code(code).unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_amount_round_trips_through_a_numeric_19_4_column(pool: PgPool) {
    let tx = Tx::new(pool);
    for (minor, code) in [
        (0, "USD"),
        (1, "USD"),
        (-1, "USD"),
        (123_456_789, "USD"),
        (-999_999_999_999_999, "USD"),
        (7, "JPY"),
        (-1, "KWD"),
        (999_999_999_999_999, "KWD"),
        (12_345, "CLF"),
    ] {
        let money = Money::from_minor(minor, currency(code));
        let back = tx
            .read(async |r| {
                sqlx::query_scalar!(r#"select $1::numeric(19,4) as "v!""#, DbAmount::from_money(money))
                    .fetch_one(r.conn())
                    .await
                    .map_err(DbError::from)
            })
            .await
            .unwrap();
        assert_eq!(back.into_money(money.currency()), Ok(money), "{money}");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_column_value_past_the_minor_unit_or_beyond_i64_is_refused(pool: PgPool) {
    let tx = Tx::new(pool);
    let excess = tx
        .read(async |r| {
            sqlx::query_scalar!(r#"select 12.345::numeric(19,4) as "v!""#)
                .fetch_one(r.conn())
                .await
                .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert_eq!(excess.into_money(currency("USD")), Err(MapError::ExcessPrecision));
    let huge = tx
        .read(async |r| {
            sqlx::query_scalar!(r#"select 123456789012345678901234.5::numeric(38,4) as "v!""#)
                .fetch_one(r.conn())
                .await
                .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert_eq!(huge.into_money(currency("USD")), Err(MapError::Overflow));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_nan_is_refused_when_it_is_read(pool: PgPool) {
    let tx = Tx::new(pool);
    let nan = tx
        .read(async |r| {
            sqlx::query_scalar!(r#"select 'NaN'::numeric as "v!""#).fetch_one(r.conn()).await.map_err(DbError::from)
        })
        .await;
    assert!(nan.is_err(), "NaN decoded as {nan:?}");
}
