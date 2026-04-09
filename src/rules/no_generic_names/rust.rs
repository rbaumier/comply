//! rust: covered by `clippy::disallowed_names`
//!
//! Configure in `clippy.toml`:
//!
//! ```toml
//! disallowed-names = ["data", "info", "temp", "result", "obj", "item"]
//! ```
//!
//! clippy's default list already includes `foo`, `baz`, `qux`, `quux`.
//! Extend it with the grab-bag names you want to reject.
//!
//! comply does not run clippy itself.
