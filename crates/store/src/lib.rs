//! Feature SQL. One module per feature, named as in `table-owners.toml`; a module writes only the tables that
//! file gives it, and any module may read any table (`scripts/check-sql.mjs`). Every statement is a checked
//! `query!`, `query_as!` or `query_scalar!`, prepared against the migrated schema when it compiles.
//! A function that writes takes `&mut db::WriteTx`; one that only reads takes `&mut impl db::Reads`.

pub mod greeting;
