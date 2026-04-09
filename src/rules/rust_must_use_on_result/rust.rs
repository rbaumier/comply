//! rust: covered by `clippy::must_use_candidate`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::must_use_candidate)]
//! ```
//!
//! This flags public functions returning Result/Option/builders that
//! are missing `#[must_use]`. The attribute forces callers to handle
//! the return value, preventing silent error discards.
//!
//! comply does not run clippy itself.
