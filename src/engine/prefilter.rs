//! Cheap pre-pass that checks whether a source file mentions any of the
//! literal substrings a rule cares about.
//!
//! Rules opt in by overriding `prefilter()` on `AstCheck` / `TextCheck`.
//! When the function returns `Some(&[...])`, the engine calls
//! [`source_matches_prefilter`] before invoking the rule's full traversal
//! and skips the rule entirely on files where none of the literals appear.

use memchr::memmem;

/// Pre-built SIMD-accelerated searchers for a rule's prefilter literals.
pub(super) type PrefilterFinders = Vec<memmem::Finder<'static>>;

/// Build `Finder` objects once from static literal slices.
pub(super) fn build_finders(literals: &'static [&'static str]) -> PrefilterFinders {
    literals
        .iter()
        .map(|lit| memmem::Finder::new(lit.as_bytes()))
        .collect()
}

/// True if `source` contains at least one of the pre-built patterns.
#[inline]
pub(super) fn source_matches_prefilter(source: &str, finders: &[memmem::Finder]) -> bool {
    let haystack = source.as_bytes();
    finders.iter().any(|f| f.find(haystack).is_some())
}

#[cfg(test)]
mod tests {
    use super::{build_finders, source_matches_prefilter};

    fn finders(lits: &'static [&'static str]) -> Vec<memchr::memmem::Finder<'static>> {
        build_finders(lits)
    }

    #[test]
    fn empty_literals_returns_false() {
        assert!(!source_matches_prefilter("anything", &finders(&[])));
    }

    #[test]
    fn single_literal_present() {
        assert!(source_matches_prefilter("x foo y", &finders(&["foo"])));
    }

    #[test]
    fn single_literal_absent() {
        assert!(!source_matches_prefilter("bar", &finders(&["foo"])));
    }

    #[test]
    fn multiple_literals_any_match() {
        assert!(source_matches_prefilter("...bar...", &finders(&["foo", "bar"])));
    }

    #[test]
    fn multiple_literals_none_match() {
        assert!(!source_matches_prefilter("baz", &finders(&["foo", "bar"])));
    }

    #[test]
    fn case_sensitive() {
        assert!(!source_matches_prefilter("foo", &finders(&["Foo"])));
    }
}

#[cfg(test)]
mod lint_in_memory_prefilter_tests {
    use crate::config::default_static_config;
    use crate::engine::lint_in_memory;
    use crate::files::Language;
    use std::path::Path;

    /// `no-eval` declares `prefilter = ["eval"]`. A TypeScript source that
    /// never mentions `eval` must produce zero diagnostics for that rule
    /// even when run through the LSP path.
    #[test]
    fn lint_in_memory_skips_rule_when_prefilter_literal_absent() {
        let source = "const x = 1;\nconst y = x + 2;\n";
        let diagnostics = lint_in_memory(
            Path::new("scratch.ts"),
            Language::TypeScript,
            source,
            default_static_config(),
            None,
        );
        assert!(
            diagnostics.iter().all(|d| d.rule_id.as_ref() != "no-eval"),
            "expected zero `no-eval` diagnostics on prefilter-miss source, got: {diagnostics:?}",
        );
    }

    /// Sanity check: when the literal IS present and the call is real,
    /// the rule still fires through `lint_in_memory`. Guards against the
    /// prefilter accidentally short-circuiting valid hits.
    #[test]
    fn lint_in_memory_runs_rule_when_prefilter_literal_present() {
        let source = "const r = eval(\"1 + 2\");\n";
        let diagnostics = lint_in_memory(
            Path::new("scratch.ts"),
            Language::TypeScript,
            source,
            default_static_config(),
            None,
        );
        assert!(
            diagnostics.iter().any(|d| d.rule_id.as_ref() == "no-eval"),
            "expected at least one `no-eval` diagnostic when source contains eval(), got: {diagnostics:?}",
        );
    }
}
