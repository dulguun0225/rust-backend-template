//! The API's tests, one test binary: the contract snapshots, the edge, the strict body sweep and the
//! worked example, each against a real PostgreSQL through `#[sqlx::test]` where it needs one.

#![cfg(test)]

mod catalog;
mod common;
mod edge;
mod greeting;
mod openapi;
mod params;
mod schemas;
mod strict_body;
