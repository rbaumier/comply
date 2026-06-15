//! Custom lint rules — each rule is a `RuleDef` with per-language backends.
//!
//! A rule concept owns a stable `RuleMeta` (id, description, remediation,
//! severity) and a list of `(Language, Backend)` pairs. The engine walks
//! every registered rule, filters by the file's language, and dispatches
//! to the matching backend.
//!
//! Backends can be:
//! - `TreeSitter` — in-process Rust AST walk (the common case for opinionated rules)
//! - `Text` — plain-text / regex / filesystem check (line count, TODO scan) // comply-ignore: todo-needs-issue-link — mention, not marker.
//! - `Oxlint` — delegation to an oxlint rule, with rule-id + message remap
//! - `Clippy` — (v2) delegation to a clippy lint
//! - `Tsc` — (v1.2) shell out to `tsc --noEmit`
//!
//! See TODO.md "Architecture" for the full rationale. // comply-ignore: todo-needs-issue-link — file reference, not marker.

use crate::diagnostic::Severity;
use crate::files::Language;
use backend::Backend;
use meta::RuleMeta;

/// A rule: identity + per-language enforcement backends.
#[derive(Debug)]
pub struct RuleDef {
    pub meta: RuleMeta,
    pub backends: Vec<(Language, Backend)>,
}

// --- helpers at src/rules/ root (not rule directories) ---
pub mod backend;
pub mod call_expression;
pub mod elysia_helpers;
pub mod file_ctx;
pub mod jsdoc_helpers;
pub mod jsdoc_text_helpers;
pub mod jsx;
pub mod meta;
pub mod meta_registry;
pub mod module_system;
pub mod object_literal;
pub mod path_utils;
pub mod playwright;
pub mod react_leak_helpers;
#[cfg(test)]
pub mod regex_ast;
mod registry;
pub use registry::build_rust_only_rule;
pub mod rust_helpers;
pub mod shell_exec_helpers;
pub mod sql_helpers;
pub mod test_assertion_helpers;
#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod test_methods;
pub mod vue_sfc;
pub mod vue_sfc_oxc;
pub mod vue_template_helpers;
pub mod walker;
pub mod yaml_k8s_helpers;

pub mod delegated;

// Generated: all `pub mod <rule>;` declarations and `pub fn all_rule_defs()`.
include!(concat!(env!("OUT_DIR"), "/generated_rules.rs"));

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
            .map(|&lang| (lang, Backend::Oxlint { rule, post_filter: None }))
            .collect(),
    }
}

/// Helper for rules bound to BOTH oxlint (TS-family) and clippy (Rust).
/// Used when the same coding standard has direct enforcement on both
/// sides: `max-depth` → oxlint `max-depth` + clippy `excessive_nesting`.
pub fn oxlint_and_clippy(
    meta: RuleMeta,
    oxlint_rule: &'static str,
    clippy_lint: &'static str,
) -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxlint { rule: oxlint_rule, post_filter: None }))
        .collect();
    backends.push((Language::Rust, Backend::Clippy { lint: clippy_lint }));
    RuleDef { meta, backends }
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
            if let Backend::Oxlint { rule: oxlint_key, .. } = backend {
                bindings.push((*oxlint_key, meta_static, meta_static.severity));
            }
        }
    }
    // Dedupe by oxlint config key (TS_FAMILY yields 3 bindings for the same key).
    bindings.sort_by_key(|(key, _, _)| *key);
    bindings.dedup_by_key(|(key, _, _)| *key);
    bindings
}

/// Accessor for the clippy-delegated backends across every registered rule.
/// Mirror of `collect_oxlint_bindings` but for `Backend::Clippy { lint }`
/// markers. Used by `crate::clippy` to generate the `-W clippy::lint`
/// flags passed to `cargo clippy` and to build the rule-id remap table.
pub fn collect_clippy_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in all_rule_defs() {
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Clippy { lint } = backend {
                bindings.push((*lint, meta_static, meta_static.severity));
            }
        }
    }
    // Dedupe by lint name — a clippy lint may be referenced by more
    // than one comply rule; keep the first binding so the clippy
    // scanner emits a single diagnostic per lint.
    bindings.sort_by_key(|(lint, _, _)| *lint);
    bindings.dedup_by_key(|(lint, _, _)| *lint);
    bindings
}

/// Accessor for tsgolint-delegated backends (type-aware rules).
/// Only used when `--type-aware` is passed.
pub fn collect_tsgolint_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in delegated::register_tsgolint() {
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Tsgolint { rule: tsgolint_key, .. } = backend {
                bindings.push((*tsgolint_key, meta_static, meta_static.severity));
            }
        }
    }
    bindings.sort_by_key(|(key, _, _)| *key);
    bindings.dedup_by_key(|(key, _, _)| *key);
    bindings
}

/// Build a map from comply rule-id to the post-filters for that rule.
///
/// Called once by the oxlint dispatcher (`crate::oxlint::lint_files`).
/// The dispatcher retains each diagnostic only when every filter in the Vec
/// returns `true` (`all()` = suppress if any filter returns `false`).
pub fn collect_delegated_post_filters(
) -> rustc_hash::FxHashMap<&'static str, Vec<std::sync::Arc<dyn backend::PostFilter>>> {
    use backend::PostFilter;
    let mut map: rustc_hash::FxHashMap<
        &'static str,
        Vec<std::sync::Arc<dyn PostFilter>>,
    > = rustc_hash::FxHashMap::default();
    let mut seen: rustc_hash::FxHashSet<&'static str> = rustc_hash::FxHashSet::default();
    for rule in all_rule_defs() {
        let rule_id = rule.meta.id;
        // Avoid processing the same comply rule more than once (TS_FAMILY
        // produces 3 backends for the same rule — break after the first match).
        if seen.contains(rule_id) {
            continue;
        }
        for (_lang, b) in &rule.backends {
            let filter_opt = match b {
                Backend::Oxlint { post_filter, .. } => post_filter.as_ref(),
                Backend::Tsgolint { post_filter, .. } => post_filter.as_ref(),
                _ => None,
            };
            if let Some(f) = filter_opt {
                map.entry(rule_id).or_default().push(std::sync::Arc::clone(f));
                seen.insert(rule_id);
                break;
            }
        }
    }
    map
}

/// Accessor for comply's custom type-aware rules (`Backend::TypeAware`).
/// Returns each rule's leaked `RuleMeta` so the sidecar phase can map the
/// rule id reported by the sidecar back to a severity and remediation.
/// Only used when `--type-aware` is passed.
pub fn collect_type_aware_bindings() -> Vec<&'static RuleMeta> {
    let mut metas = Vec::new();
    for rule in delegated::register_type_aware() {
        if rule
            .backends
            .iter()
            .any(|(_, backend)| matches!(backend, Backend::TypeAware))
        {
            metas.push(&*Box::leak(Box::new(rule.meta)));
        }
    }
    metas
}

static RULE_DEFS: std::sync::OnceLock<Vec<RuleDef>> = std::sync::OnceLock::new();

/// Returns the global, lazily-initialised list of all rule definitions.
/// Initialised on first call; subsequent calls are lock-free reads.
pub fn all_rule_defs_static() -> &'static [RuleDef] {
    RULE_DEFS.get_or_init(all_rule_defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_count_and_unique_ids() {
        let rules = all_rule_defs();
        assert!(rules.len() >= 1845, "expected ≥1845 rules, got {}", rules.len());
        let mut ids: Vec<_> = rules.iter().map(|r| r.meta.id).collect();
        ids.sort();
        let mut deduped = ids.clone();
        deduped.dedup();
        if deduped.len() != ids.len() {
            let mut seen = std::collections::HashSet::new();
            let dups: Vec<_> = ids.iter().filter(|id| !seen.insert(*id)).collect();
            panic!("duplicate rule IDs detected: {:?}", dups);
        }
    }
}
