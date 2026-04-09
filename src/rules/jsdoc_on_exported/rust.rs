//! rust: covered by the built-in `missing_docs` rustc lint
//!
//! rustc has a built-in `missing_docs` lint that flags every public item
//! without a doc comment. Enable it at the crate root:
//!
//! ```ignore
//! #![warn(missing_docs)]
//! ```
//!
//! For stricter enforcement, also enable
//! `clippy::missing_docs_in_private_items`:
//!
//! ```ignore
//! #![warn(clippy::missing_docs_in_private_items)]
//! ```
//!
//! Rust's convention is `/// doc comment` directly above the `pub fn` /
//! `pub struct` / `pub mod`, which is exactly what comply's TypeScript
//! rule enforces for JSDoc blocks.
//!
//! comply does not run rustc or clippy itself.
