//! rust: covered by the clippy "restriction" group
//!
//! Rust has no `throw` keyword — the equivalent is `panic!()` and the
//! family of macros/methods that abort execution. clippy ships dedicated
//! lints for every one of them:
//!
//! ```ignore
//! #![warn(
//!     clippy::panic,
//!     clippy::unwrap_used,
//!     clippy::expect_used,
//!     clippy::todo,
//!     clippy::unimplemented,
//!     clippy::unreachable,
//! )]
//! ```
//!
//! These are in the clippy `restriction` group and are off by default.
//! Enable them at the crate root for Result-first codebases. comply does
//! not run clippy itself — your project's `cargo clippy` pass enforces
//! these rules.
