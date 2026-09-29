//! The foundation tier: what every other crate may use and what uses no other first-party crate. No HTTP and
//! no SQL; `layering.toml` holds that.
//!
//! - [`clock`]: the injected clock; the only wall-clock read is [`clock::SystemClock`].
//! - [`ids`]: UUIDv7, the only id producer.
//! - [`log`]: the one typed logging facade; the only user of the `tracing` event macros.
//! - [`catalog`]: the compile-checked wire-code catalogs every error response and field error draws from.
//! - [`pager`]: the keyset pager's opaque cursor and page assembly.

pub mod catalog;
pub mod clock;
pub mod ids;
pub mod log;
pub mod pager;
