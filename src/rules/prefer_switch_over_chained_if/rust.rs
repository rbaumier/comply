//! rust: partially covered by `clippy::comparison_chain`
//!
//! clippy's `comparison_chain` flags a chain of `if x < 0 { ... } else if
//! x == 0 { ... } else { ... }` and suggests `match x.cmp(&0)`. It
//! catches the classic 3-arm numeric comparison chain but not arbitrary
//! 4+ arm `if/else if` chains on the same discriminant.
//!
//! Enable:
//!
//! ```ignore
//! #![warn(clippy::comparison_chain)]
//! ```
//!
//! For the broader "long chain → use `match`" case, rely on the language's
//! idiom: Rust programmers already reach for `match` naturally. comply's
//! TypeScript rule exists because TS has no pattern-matching keyword.
//!
//! comply does not run clippy itself.
