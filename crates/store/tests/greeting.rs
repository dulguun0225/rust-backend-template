//! The greeting store against a real PostgreSQL: a fresh database per test, migrated by `db::MIGRATOR`.

#![cfg(test)]

use db::Tx;
use db::versioned::Outcome;
use platform::ids::new_id;
use sqlx::PgPool;
use time::Duration;
use time::macros::datetime;

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_inserted_greeting_reads_back(pool: PgPool) {
    let tx = Tx::new(pool);
    let id = new_id();
    let at = datetime!(2026-09-29 10:00:00.123456 UTC);
    tx.write(async |w| store::greeting::insert(w, id, "Ada", at).await).await.unwrap();
    let row = tx.read(async |r| store::greeting::find(r, id).await).await.unwrap().unwrap();
    assert_eq!((row.id, row.name.as_str(), row.created_at, row.version), (id, "Ada", at, 1));
    assert_eq!(tx.read(async |r| store::greeting::find(r, new_id()).await).await.unwrap(), None);
    assert_eq!(tx.begun(), 3);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_read_transaction_refuses_a_write(pool: PgPool) {
    let tx = Tx::new(pool);
    let result: Result<(), db::DbError> = tx
        .read(async |r| {
            let at = datetime!(2026-09-29 10:00 UTC);
            sqlx::query!("insert into greeting (id, name, created_at, version) values ($1, 'x', $2, 1)", new_id(), at)
                .execute(db::Reads::conn(r))
                .await?;
            Ok(())
        })
        .await;
    let message = result.unwrap_err().to_string();
    assert!(message.contains("read-only transaction"), "{message}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_guarded_update_classifies_applied_stale_and_absent(pool: PgPool) {
    let tx = Tx::new(pool);
    let id = new_id();
    tx.write(async |w| store::greeting::insert(w, id, "Ada", datetime!(2026-09-29 10:00 UTC)).await).await.unwrap();

    let applied = tx.write(async |w| store::greeting::rename(w, id, 1, "Grace").await).await.unwrap();
    assert_eq!(applied, Outcome::Applied { new_version: 2 });
    let stale = tx.write(async |w| store::greeting::rename(w, id, 1, "Edsger").await).await.unwrap();
    assert_eq!(stale, Outcome::Stale { current_version: 2 });
    let absent = tx.write(async |w| store::greeting::rename(w, new_id(), 1, "Barbara").await).await.unwrap();
    assert_eq!(absent, Outcome::Absent);

    let row = tx.read(async |r| store::greeting::find(r, id).await).await.unwrap().unwrap();
    assert_eq!((row.name.as_str(), row.version), ("Grace", 2));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pages_walk_newest_first_without_gaps_or_repeats(pool: PgPool) {
    let tx = Tx::new(pool);
    let start = datetime!(2026-09-29 10:00 UTC);
    let mut ids = Vec::new();
    // two rows share each instant, so the id tiebreak decides their order
    for minute in [0, 0, 1, 1, 2, 2, 3] {
        let id = new_id();
        let at = start.checked_add(Duration::minutes(minute)).unwrap();
        tx.write(async |w| store::greeting::insert(w, id, "row", at).await).await.unwrap();
        ids.push((at, id));
    }
    ids.sort_by(|a, b| b.cmp(a));

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = tx.read(async |r| store::greeting::page(r, cursor, 3).await).await.unwrap();
        seen.extend(page.items.iter().map(|row| (row.created_at, row.id)));
        match page.next_cursor {
            Some(next) => cursor = platform::pager::decode(Some(&next)).unwrap(),
            None => break,
        }
    }
    assert_eq!(seen, ids);
}
