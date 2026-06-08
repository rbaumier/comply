//! Boot-time lookup table: `rule_id` → `RuleMeta`. Built once on first call
//! via `OnceLock` from `all_rule_defs()`. The pretty renderer uses this to
//! surface RuleMeta-only fields (description, remediation, doc_url) that
//! aren't carried on `Diagnostic` itself.
//!
//! Delegated diagnostics (oxlint, clippy, knip, madge) carry rule ids that
//! are NOT in comply's RuleMeta catalogue — `lookup` returns `None` for
//! those, and the renderer omits the help/url sections for that diagnostic.

use crate::rules::meta::RuleMeta;
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

static REGISTRY: OnceLock<FxHashMap<&'static str, RuleMeta>> = OnceLock::new();

fn build() -> FxHashMap<&'static str, RuleMeta> {
    crate::rules::all_rule_defs()
        .into_iter()
        .map(|r| (r.meta.id, r.meta))
        .collect()
}

/// Returns the `RuleMeta` for a given rule id, or `None` if the id is
/// unknown to comply (typically a delegated oxlint/clippy id).
#[must_use]
pub fn lookup(rule_id: &str) -> Option<RuleMeta> {
    REGISTRY.get_or_init(build).get(rule_id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_registered_rule() {
        // "no-weak-cipher" is a real rule registered in all_rule_defs().
        let meta = lookup("no-weak-cipher").expect("no-weak-cipher must be in the registry");
        assert_eq!(meta.id, "no-weak-cipher");
    }

    #[test]
    fn lookup_returns_none_for_unknown_rule_id() {
        assert!(lookup("not-a-real-rule-id-zzz-xyz").is_none());
    }

    #[test]
    fn lookup_is_memoized_across_calls() {
        // Two successive calls should hit the same OnceLock instance and
        // return equal RuleMeta (observed via id equality since RuleMeta
        // itself isn't PartialEq).
        let a = lookup("no-weak-cipher").unwrap();
        let b = lookup("no-weak-cipher").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.description, b.description);
    }
}
