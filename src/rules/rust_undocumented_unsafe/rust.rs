//! rust: covered by `clippy::undocumented_unsafe_blocks`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::undocumented_unsafe_blocks)]
//! ```
//!
//! Every `unsafe { ... }` block needs a `// SAFETY: ...` comment
//! explaining the invariants the caller relies on. This lint enforces
//! the comment's presence.
//!
//! comply does not run clippy itself.
