//! rust: partially covered by the built-in `missing_docs` rustc lint
//!
//! rustc's `missing_docs` lint flags modules (including the crate root)
//! that lack a top-level `//! ...` doc comment. It enforces PRESENCE but
//! not the "What / How" structural convention comply's TypeScript rule
//! requires.
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(missing_docs)]
//! ```
//!
//! For the stricter "what it does + how it works" narrative, rely on
//! code review — no automated lint covers that rhetorical structure.
//!
//! comply does not run rustc itself.
