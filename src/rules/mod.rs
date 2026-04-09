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
pub mod delegated;
pub mod max_file_lines;
pub mod max_function_lines;
pub mod meta;
pub mod no_nested_ternary;
pub mod no_throw;
pub mod walker;

use crate::diagnostic::Severity;
use crate::files::Language;
use backend::Backend;
use meta::RuleMeta;

/// A rule: identity + per-language enforcement backends.
pub struct RuleDef {
    pub meta: RuleMeta,
    pub backends: Vec<(Language, Backend)>,
}

/// Language slice for the TS-family. Used by rules that apply to all three
/// variants identically (either via the TS grammar or oxlint delegation).
pub const TS_FAMILY: &[Language] = &[Language::TypeScript, Language::Tsx, Language::JavaScript];

/// Helper for rules whose enforcement is 100% delegated to oxlint.
/// Each entry in `languages` gets a `Backend::Oxlint { rule }` binding.
pub fn oxlint_delegate(meta: RuleMeta, rule: &'static str, languages: &[Language]) -> RuleDef {
    RuleDef {
        meta,
        backends: languages
            .iter()
            .map(|&lang| (lang, Backend::Oxlint { rule }))
            .collect(),
    }
}

/// Accessor for the oxlint-delegated backends across every registered rule.
/// Used by the oxlint subprocess module to generate the runtime config and
/// build the diagnostic-code remap table.
pub fn collect_oxlint_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in all_rule_defs() {
        // Leak the meta to 'static so the caller can reference it across the
        // oxlint subprocess boundary without lifetime gymnastics. This runs
        // once per process invocation, so the leak is negligible.
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Oxlint { rule: oxlint_key } = backend {
                bindings.push((*oxlint_key, meta_static, meta_static.severity));
            }
        }
    }
    // Dedupe by oxlint config key (TS_FAMILY yields 3 bindings for the same key).
    bindings.sort_by_key(|(key, _, _)| *key);
    bindings.dedup_by_key(|(key, _, _)| *key);
    bindings
}

/// All registered rules — both the custom ones and the oxlint-delegated ones.
pub fn all_rule_defs() -> Vec<RuleDef> {
    let mut rules = vec![
        max_file_lines::register(),
        max_function_lines::register(),
        no_throw::register(),
        no_nested_ternary::register(),
        banned_identifiers::register(),
    ];
    rules.extend(delegated::register_all());
    rules
}
