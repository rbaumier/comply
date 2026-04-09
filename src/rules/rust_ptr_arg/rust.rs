//! rust: covered by `clippy::ptr_arg`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::ptr_arg)]
//! ```
//!
//! Prefer borrowed slices over borrowed owned types in function
//! parameters: `&str` over `&String`, `&[T]` over `&Vec<T>`,
//! `&Path` over `&PathBuf`. The slice form accepts more caller types
//! and avoids the extra indirection.
//!
//! comply does not run clippy itself.
