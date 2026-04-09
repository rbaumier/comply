//! rust: covered by `clippy::unwrap_used` + `clippy::expect_used`
//!
//! Enable both at the crate root:
//!
//! ```ignore
//! #![warn(clippy::unwrap_used, clippy::expect_used)]
//! ```
//!
//! Both lints are in the clippy `restriction` group and are off by
//! default. Turn them on for production code where every error must
//! be handled via Result instead of panicking.
//!
//! comply does not run clippy itself. This rule documents the mapping
//! so the coding standard is visible in `comply list` and `comply explain`.
