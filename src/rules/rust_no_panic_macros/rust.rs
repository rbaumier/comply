//! rust: covered by `clippy::panic` + `clippy::todo` +
//! `clippy::unimplemented` + `clippy::unreachable`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(
//!     clippy::panic,
//!     clippy::todo,
//!     clippy::unimplemented,
//!     clippy::unreachable,
//! )]
//! ```
//!
//! All four are in the clippy `restriction` group. Turn them on for
//! production code where panics must only originate from verified
//! invariant violations, not from placeholder macros.
//!
//! comply does not run clippy itself.
