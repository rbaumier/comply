//! rust: covered by `clippy::linkedlist`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::linkedlist)]
//! ```
//!
//! `Vec<T>` dominates `LinkedList<T>` for any realistic N. Cache
//! locality beats theoretical O(1) splice operations in practice —
//! LinkedList is almost never the right choice.
//!
//! comply does not run clippy itself.
