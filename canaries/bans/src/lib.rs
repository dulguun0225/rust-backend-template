//! Ban canaries: one committed violation per entry of the root clippy.toml and per lint the workspace
//! denies or forbids. scripts/check-ban-canaries.mjs compiles this file in a workspace generated from the root
//! Cargo.toml (same dependencies, same features, same lints, same clippy.toml) and fails unless clippy reports,
//! on each marked line, exactly the diagnostics its `// expect:` comment names, and nothing on any other line.
//! A misspelled ban path is only a warning and a path whose crate is missing is ignored without a word, so
//! an entry with no canary here is an entry nothing proves is live.
//!
//! This file is never part of the workspace build.

use std::time::Duration;

// ---- disallowed-methods: blocking calls ----

pub fn blocking() {
    std::thread::sleep(Duration::ZERO); // expect: ban:std::thread::sleep
    let _a = std::fs::read("x"); // expect: ban:std::fs::read
    let _b = std::fs::read_to_string("x"); // expect: ban:std::fs::read_to_string
    let _c = std::fs::write("x", "y"); // expect: ban:std::fs::write
    let _d = std::fs::read_dir("x"); // expect: ban:std::fs::read_dir
    let _e = std::fs::create_dir("x"); // expect: ban:std::fs::create_dir
    let _f = std::fs::create_dir_all("x"); // expect: ban:std::fs::create_dir_all
    let _g = std::fs::remove_file("x"); // expect: ban:std::fs::remove_file
    let _h = std::fs::remove_dir("x"); // expect: ban:std::fs::remove_dir
    let _i = std::fs::remove_dir_all("x"); // expect: ban:std::fs::remove_dir_all
    let _j = std::fs::copy("x", "y"); // expect: ban:std::fs::copy
    let _k = std::fs::rename("x", "y"); // expect: ban:std::fs::rename
    let _l = std::fs::metadata("x"); // expect: ban:std::fs::metadata
    let _m = std::fs::symlink_metadata("x"); // expect: ban:std::fs::symlink_metadata
    let _n = std::fs::canonicalize("x"); // expect: ban:std::fs::canonicalize
    let _o = std::fs::exists("x"); // expect: ban:std::fs::exists
    let _s = std::fs::hard_link("x", "y"); // expect: ban:std::fs::hard_link
    let _t = std::fs::read_link("x"); // expect: ban:std::fs::read_link
    let _u = std::fs::metadata("x").map(|m| std::fs::set_permissions("x", m.permissions())); // expect: ban:std::fs::metadata ban:std::fs::set_permissions
    let _p = std::fs::File::open("x"); // expect: ban:std::fs::File::open
    let _q = std::fs::File::create("x"); // expect: ban:std::fs::File::create
    let _v = std::fs::File::create_new("x"); // expect: ban:std::fs::File::create_new
    let _r = std::fs::OpenOptions::new().open("x"); // expect: ban:std::fs::OpenOptions::open
    let _w = std::fs::DirBuilder::new().create("x"); // expect: ban:std::fs::DirBuilder::create
}

// ---- disallowed-methods: clock and ids ----

pub fn clock_and_ids() {
    let _a = std::time::SystemTime::now(); // expect: ban:std::time::SystemTime::now
    let _b = time::OffsetDateTime::now_utc(); // expect: ban:time::OffsetDateTime::now_utc
    let _c = time::UtcDateTime::now(); // expect: ban:time::UtcDateTime::now
    let _d = uuid::Uuid::now_v7(); // expect: ban:uuid::Uuid::now_v7
    let _e = uuid::Uuid::new_v7(uuid::Timestamp::from_unix_time(0, 0, 0, 0)); // expect: ban:uuid::Uuid::new_v7
    let _f = std::time::UNIX_EPOCH.elapsed(); // expect: ban:std::time::SystemTime::elapsed
    let _g = uuid::Timestamp::now(uuid::NoContext); // expect: ban:uuid::Timestamp::now
}

// ---- disallowed-methods: unchecked SQL and transactions outside db::Tx ----

pub async fn sql(pool: &sqlx::PgPool, conn: &mut sqlx::PgConnection) {
    use sqlx::Executor as _;
    let _a = sqlx::query::<sqlx::Postgres>("select 1"); // expect: ban:sqlx::query
    let _b = sqlx::query_as::<sqlx::Postgres, (i32,)>("select 1"); // expect: ban:sqlx::query_as
    let _c = sqlx::query_scalar::<sqlx::Postgres, i32>("select 1"); // expect: ban:sqlx::query_scalar
    let _d = sqlx::query_with("select 1", sqlx::postgres::PgArguments::default()); // expect: ban:sqlx::query_with
    let _e = sqlx::query_as_with::<_, (i32,), _>("select 1", sqlx::postgres::PgArguments::default()); // expect: ban:sqlx::query_as_with
    let _f = sqlx::query_scalar_with::<_, i32, _>("select 1", sqlx::postgres::PgArguments::default()); // expect: ban:sqlx::query_scalar_with
    let _g = sqlx::raw_sql("select 1"); // expect: ban:sqlx::raw_sql
    let _h = pool.execute("select 1").await; // expect: ban:sqlx::Executor::execute
    let _i = pool.execute_many("select 1"); // expect: ban:sqlx::Executor::execute_many
    let _j = pool.fetch("select 1"); // expect: ban:sqlx::Executor::fetch
    let _k = pool.fetch_many("select 1"); // expect: ban:sqlx::Executor::fetch_many
    let _l = pool.fetch_all("select 1").await; // expect: ban:sqlx::Executor::fetch_all
    let _m = pool.fetch_one("select 1").await; // expect: ban:sqlx::Executor::fetch_one
    let _n = pool.fetch_optional("select 1").await; // expect: ban:sqlx::Executor::fetch_optional
    let _o = pool.begin().await; // expect: ban:sqlx::Pool::begin
    let _p = pool.begin_with("BEGIN").await; // expect: ban:sqlx::Pool::begin_with
    let _q = pool.try_begin().await; // expect: ban:sqlx::Pool::try_begin
    let _r = pool.acquire().await; // expect: ban:sqlx::Pool::acquire
    let _s = pool.try_acquire(); // expect: ban:sqlx::Pool::try_acquire
    let _t = sqlx::Connection::begin(conn).await; // expect: ban:sqlx::Connection::begin
    let _u = sqlx::Acquire::begin(pool).await; // expect: ban:sqlx::Acquire::begin
    let _v = sqlx::Acquire::acquire(pool).await; // expect: ban:sqlx::Acquire::acquire
    let _w = sqlx::PgPool::connect("x").await; // expect: ban:sqlx::Pool::connect
    let _x = sqlx::PgPool::connect_with(sqlx::postgres::PgConnectOptions::new()).await; // expect: ban:sqlx::Pool::connect_with
    let _y = sqlx::PgPool::connect_lazy("x"); // expect: ban:sqlx::Pool::connect_lazy
    let _z = sqlx::PgPool::connect_lazy_with(sqlx::postgres::PgConnectOptions::new()); // expect: ban:sqlx::Pool::connect_lazy_with
    let options = sqlx::postgres::PgPoolOptions::new;
    let _pa = options().connect("x").await; // expect: ban:sqlx::pool::PoolOptions::connect
    let _pb = options().connect_with(sqlx::postgres::PgConnectOptions::new()).await; // expect: ban:sqlx::pool::PoolOptions::connect_with
    let _pc = options().connect_lazy("x"); // expect: ban:sqlx::pool::PoolOptions::connect_lazy
    let _pd = options().connect_lazy_with(sqlx::postgres::PgConnectOptions::new()); // expect: ban:sqlx::pool::PoolOptions::connect_lazy_with
    let _ca = <sqlx::PgConnection as sqlx::Connection>::connect("x").await; // expect: ban:sqlx::Connection::connect
    let _cb = <sqlx::PgConnection as sqlx::Connection>::connect_with(&sqlx::postgres::PgConnectOptions::new()).await; // expect: ban:sqlx::Connection::connect_with
}

// ---- disallowed-methods: routes outside OpenApiRouter::routes ----

async fn handler() -> &'static str {
    "x"
}

pub fn routing() {
    let _a = axum::Router::<()>::new().route("/a", axum::routing::get(handler)); // expect: ban:axum::Router::route
    let _b = axum::Router::<()>::new().route_service("/b", axum::routing::get(handler)); // expect: ban:axum::Router::route_service
    let _c = axum::Router::<()>::new().nest("/c", axum::Router::new()); // expect: ban:axum::Router::nest
    let _d = axum::Router::<()>::new().nest_service("/d", axum::routing::get(handler)); // expect: ban:axum::Router::nest_service
    let _e = axum::Router::<()>::new().merge(axum::Router::new()); // expect: ban:axum::Router::merge
    let _f = axum::Router::<()>::new().fallback(handler); // expect: ban:axum::Router::fallback
    let _g = axum::Router::<()>::new().fallback_service(axum::routing::get(handler)); // expect: ban:axum::Router::fallback_service
    let open = utoipa_axum::router::OpenApiRouter::<()>::new;
    let _h = open().route("/h", axum::routing::get(handler)); // expect: ban:utoipa_axum::router::OpenApiRouter::route
    let _i = open().route_service("/i", axum::routing::get(handler)); // expect: ban:utoipa_axum::router::OpenApiRouter::route_service
    let _j = open().nest_service("/j", axum::routing::get(handler)); // expect: ban:utoipa_axum::router::OpenApiRouter::nest_service
    let _k = open().fallback(handler); // expect: ban:utoipa_axum::router::OpenApiRouter::fallback
    let _l = open().fallback_service(axum::routing::get(handler)); // expect: ban:utoipa_axum::router::OpenApiRouter::fallback_service
}

// ---- disallowed-methods: money division and silent wrap ----

pub fn money(a: i64, b: i64, c: i128, d: i128, v: &[i64]) {
    let _a = a.checked_div(b); // expect: ban:i64::checked_div
    let _b = a.checked_rem(b); // expect: ban:i64::checked_rem
    let _c = a.div_euclid(b); // expect: ban:i64::div_euclid
    let _d = a.rem_euclid(b); // expect: ban:i64::rem_euclid
    let _e = a.checked_div_euclid(b); // expect: ban:i64::checked_div_euclid
    let _f = a.checked_rem_euclid(b); // expect: ban:i64::checked_rem_euclid
    let _g = c.checked_div(d); // expect: ban:i128::checked_div
    let _h = c.checked_rem(d); // expect: ban:i128::checked_rem
    let _i = c.div_euclid(d); // expect: ban:i128::div_euclid
    let _j = c.rem_euclid(d); // expect: ban:i128::rem_euclid
    let _k = c.checked_div_euclid(d); // expect: ban:i128::checked_div_euclid
    let _l = c.checked_rem_euclid(d); // expect: ban:i128::checked_rem_euclid
    let _m: i64 = v.iter().sum(); // expect: ban:core::iter::Iterator::sum
    let _n: i64 = v.iter().product(); // expect: ban:core::iter::Iterator::product
}

// ---- disallowed-methods: anyhow outside the binary, bodies outside StrictJson ----

pub fn anyhow_context(v: Option<u8>) {
    use anyhow::Context as _;
    let _a = v.context("x"); // expect: ban:anyhow::Context::context
    let _b = v.with_context(|| "x"); // expect: ban:anyhow::Context::with_context
}

pub async fn bodies(bytes: &[u8], body: axum::body::Body) {
    let _a = serde_json::from_slice::<u8>(bytes); // expect: ban:serde_json::from_slice
    let _b = serde_json::from_str::<u8>("1"); // expect: ban:serde_json::from_str
    let _c = serde_json::from_reader::<_, u8>(bytes); // expect: ban:serde_json::from_reader
    let _d = serde_json::from_value::<u8>(serde_json::Value::Null); // expect: ban:serde_json::from_value
    let _e = axum::body::to_bytes(body, 1).await; // expect: ban:axum::body::to_bytes
}

// ---- disallowed-types ----

pub struct Types {
    pub a: anyhow::Error, // expect: ban:anyhow::Error
    pub b: anyhow::Result<u8>, // expect: ban:anyhow::Result
    pub c: bigdecimal::BigDecimal, // expect: ban:bigdecimal::BigDecimal
    pub d: sqlx::AssertSqlSafe<String>, // expect: ban:sqlx::AssertSqlSafe
    pub e: sqlx::QueryBuilder<sqlx::Postgres>, // expect: ban:sqlx::QueryBuilder
    pub f: axum::extract::RawForm, // expect: ban:axum::extract::RawForm
    pub g: axum::body::Bytes, // expect: ban:axum::body::Bytes
    pub h: uuid::Builder, // expect: ban:uuid::Builder
}

// ---- disallowed-macros ----

pub fn anyhow_macros() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _a = anyhow::anyhow!("x"); // expect: ban:anyhow::anyhow
    ensure_ok()?;
    bail_now()?;
    Ok(())
}

fn ensure_ok() -> Result<(), anyhow::Error> { // expect: ban:anyhow::Error
    anyhow::ensure!(true, "x"); // expect: ban:anyhow::ensure
    Ok(())
}

fn bail_now() -> Result<(), anyhow::Error> { // expect: ban:anyhow::Error
    anyhow::bail!("x") // expect: ban:anyhow::bail
}

pub async fn unchecked_sql(conn: &mut sqlx::PgConnection) {
    let _a = sqlx::query_unchecked!("select 1 as one").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_unchecked
    let _b = sqlx::query_as_unchecked!(One, "select 1 as one").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_as_unchecked
    let _c = sqlx::query_scalar_unchecked!("select 1 as one").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_scalar_unchecked
    let _d = sqlx::query_file!("sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file
    let _e = sqlx::query_file_as!(One, "sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file_as
    let _f = sqlx::query_file_scalar!("sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file_scalar
    let _g = sqlx::query_file_unchecked!("sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file_unchecked
    let _h = sqlx::query_file_as_unchecked!(One, "sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file_as_unchecked
    let _i = sqlx::query_file_scalar_unchecked!("sql/canary.sql").fetch_one(&mut *conn).await; // expect: ban:sqlx::query_file_scalar_unchecked
}

pub struct One {
    pub one: Option<i32>,
}

pub fn tracing_macros() {
    tracing::trace!("x"); // expect: ban:tracing::trace ban:tracing::event
    tracing::debug!("x"); // expect: ban:tracing::debug ban:tracing::event
    tracing::info!("x"); // expect: ban:tracing::info ban:tracing::event
    tracing::warn!("x"); // expect: ban:tracing::warn ban:tracing::event
    tracing::error!("x"); // expect: ban:tracing::error ban:tracing::event
    tracing::event!(tracing::Level::INFO, "x"); // expect: ban:tracing::event
    let _a = tracing::span!(tracing::Level::INFO, "x"); // expect: ban:tracing::span
    let _b = tracing::trace_span!("x"); // expect: ban:tracing::trace_span ban:tracing::span
    let _c = tracing::debug_span!("x"); // expect: ban:tracing::debug_span ban:tracing::span
    let _d = tracing::info_span!("x"); // expect: ban:tracing::info_span ban:tracing::span
    let _e = tracing::warn_span!("x"); // expect: ban:tracing::warn_span ban:tracing::span
    let _f = tracing::error_span!("x"); // expect: ban:tracing::error_span ban:tracing::span
}

#[tracing::instrument] // expect: ban:tracing::instrument ban:tracing::span
pub fn instrumented() {}

// ---- workspace lints: forbid ----

pub enum Two {
    A,
    B,
}

pub enum Three {
    A,
    B,
    C,
}

#[must_use]
pub fn must() -> u8 {
    1
}

pub fn forbidden(a: i64, b: i64, u: u64, x: f64, v: &[u8], opt: Option<u8>) {
    let _a = opt.unwrap(); // expect: clippy::unwrap_used
    if a == 7 {
        panic!("x"); // expect: clippy::panic
    }
    if a == 8 {
        todo!(); // expect: clippy::todo
    }
    if a == 9 {
        unimplemented!(); // expect: clippy::unimplemented
    }
    let _b = v[0]; // expect: clippy::indexing_slicing
    let _c = a + b; // expect: clippy::arithmetic_side_effects
    let _d = a / 2; // expect: clippy::integer_division clippy::integer_division_remainder_used
    let _e = a % 2; // expect: clippy::integer_division_remainder_used
    let _f = a as u8; // expect: clippy::as_conversions clippy::cast_possible_truncation clippy::cast_sign_loss
    let _g = a as f64; // expect: clippy::as_conversions clippy::cast_precision_loss
    let _h = u as i64; // expect: clippy::as_conversions clippy::cast_possible_wrap
    let _i = x * 2.0; // expect: clippy::float_arithmetic
    let _j = x == 1.5; // expect: clippy::float_cmp
    let _ = must(); // expect: clippy::let_underscore_must_use
    let _l = dbg!(a); // expect: clippy::dbg_macro
    println!("x"); // expect: clippy::print_stdout
    eprintln!("x"); // expect: clippy::print_stderr
    std::mem::forget(String::new()); // expect: clippy::mem_forget
}

pub fn wildcards(two: &Two, three: &Three) {
    let _k = match three {
        Three::A => 1,
        _ => 2, // expect: clippy::wildcard_enum_match_arm
    };
    let _m = match two {
        Two::A => 1,
        _ => 2, // expect: clippy::match_wildcard_for_single_variants
    };
}

#[allow(dead_code)] // expect: clippy::allow_attributes
fn allowed() {}

// ---- workspace lints: deny ----

pub async fn denied(m: &std::sync::Mutex<u8>, span: &tracing::Span, opt: Option<u8>) {
    let _a = opt.expect("x"); // expect: clippy::expect_used
    if m.try_lock().is_err() {
        unreachable!(); // expect: clippy::unreachable
    }
    let guard = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner); // expect: clippy::await_holding_lock
    handler().await;
    drop(guard);
    let entered = span.enter(); // expect: ban:tracing::span::Entered
    handler().await;
    drop(entered);
    let owned = span.clone().entered(); // expect: ban:tracing::span::EnteredSpan
    handler().await;
    drop(owned);
}

pub fn rust_lints() {
    let _a = unsafe { std::ptr::read(&0_u8) }; // expect: unsafe_code
    must(); // expect: unused_must_use
    let unused = 1; // expect: unused_variables
}
