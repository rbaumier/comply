//! rust: partially covered by `clippy::disallowed_names`
//!
//! clippy provides a configurable banned-name list via `clippy.toml`:
//!
//! ```toml
//! disallowed-names = [
//!     "process", "handle", "data", "do",
//!     "execute", "run", "perform",
//! ]
//! ```
//!
//! Note: `disallowed_names` matches EXACT identifiers, not prefixes with
//! camelCase/snake_case boundaries like comply's TypeScript rule. A Rust
//! function named `handle_click` won't be caught by default — you'd need
//! to extend the list or write a project-specific clippy lint.
//!
//! comply does not run clippy itself.
