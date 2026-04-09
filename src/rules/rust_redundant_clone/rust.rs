//! rust: covered by `clippy::redundant_clone`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::redundant_clone)]
//! ```
//!
//! Clones have a cost. Clippy's `redundant_clone` flags calls to
//! `.clone()` whose result is never observed separately from the
//! original — the clone adds allocation and copy work for no benefit.
//!
//! comply does not run clippy itself.
