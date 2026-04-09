//! rust: covered by `clippy::needless_range_loop` + `clippy::explicit_iter_loop`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::needless_range_loop, clippy::explicit_iter_loop)]
//! ```
//!
//! Iterator chains (`for x in &vec`) are preferred over index loops
//! (`for i in 0..vec.len() { vec[i] }`). The compiler vectorizes
//! iterators more aggressively, and the code reads cleaner.
//!
//! comply does not run clippy itself.
