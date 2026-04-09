//! Custom lint rules — each rule is a `RuleDef` with per-language backends.
//!
//! A rule concept owns a stable `RuleMeta` (id, description, remediation,
//! severity) and a list of `(Language, Backend)` pairs. The engine walks
//! every registered rule, filters by the file's language, and dispatches
//! to the matching backend.
//!
//! Backends can be:
//! - `TreeSitter` — in-process Rust AST walk (the common case for opinionated rules)
//! - `Text` — plain-text / regex / filesystem check (line count, TODO scan)
//! - `Oxlint` — delegation to an oxlint rule, with rule-id + message remap
//! - `Clippy` — (v2) delegation to a clippy lint
//! - `Tsc` — (v1.2) shell out to `tsc --noEmit`
//!
//! See TODO.md "Architecture" for the full rationale.

pub mod backend;
pub mod banned_identifiers;
pub mod max_file_lines;
pub mod max_function_lines;
pub mod meta;
pub mod no_nested_ternary;
pub mod no_throw;
pub mod walker;

use crate::files::Language;

/// A rule: identity + per-language enforcement backends.
pub struct RuleDef {
    pub meta: meta::RuleMeta,
    pub backends: Vec<(Language, backend::Backend)>,
}

/// All registered rules.
pub fn all_rule_defs() -> Vec<RuleDef> {
    vec![
        max_file_lines::register(),
        max_function_lines::register(),
        no_throw::register(),
        no_nested_ternary::register(),
        banned_identifiers::register(),
    ]
}
