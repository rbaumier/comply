//! RuleMeta — the stable identity card of a lint rule.
//!
//! Every concrete rule is a RuleMeta + one or more per-language backends.
//! The meta carries:
//! - the user-visible rule id (stable across releases)
//! - the human-readable description
//! - the remediation message (what ends up in the diagnostic output)
//! - the default severity
//! - an optional doc URL for deeper context
//!
//! Keeping meta separate from the backends lets a single concept be enforced
//! by different mechanisms per language (tree-sitter for TS, clippy for Rust,
//! oxlint for some JS rules) without fragmenting the user-facing id.

use std::path::Path;

use crate::config::Config;
use crate::diagnostic::Severity;
use crate::rules::file_ctx::FileCtx;

/// Stable identity + presentation for a lint rule.
///
/// The engine currently dispatches solely on the backends and uses the
/// backend-embedded `rule_id` string in each diagnostic. The RuleMeta is
/// carried alongside every rule so future features (JSON output with rule
/// metadata, `comply explain <rule>`, Oxlint diagnostic remapping) can
/// surface description/remediation/doc_url without re-plumbing.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Fields read by JSON output / explain / remap (coming soon).
// comply-ignore: rust-impl-debug-on-public-types — false positive: the derive above includes Debug; the rule's walker misses it through the multi-attribute block.
pub struct RuleMeta {
    /// Stable id shown in diagnostics (e.g. "no-default-params").
    pub id: &'static str,
    /// One-line summary.
    pub description: &'static str,
    /// Full remediation message emitted in diagnostics. Written as a
    /// step-by-step fix the reader can act on directly.
    pub remediation: &'static str,
    /// Default severity — rules may downgrade/upgrade per backend if needed.
    pub severity: Severity,
    /// Optional link to the rule's documentation.
    pub doc_url: Option<&'static str>,
    /// Hierarchical categories — general to specific (e.g. `&["typescript", "react"]`).
    /// Used by `comply catalog` to group rules by domain.
    pub categories: &'static [&'static str],
    /// When true, this rule is not applied inside test directories
    /// (`__tests__/`, `*.test.*`, `*.spec.*`, etc.).
    pub skip_in_test_dir: bool,
    /// When true, this rule is not applied inside relaxed directories
    /// (`examples/`, etc.) where stricter conventions are intentionally relaxed.
    pub skip_in_relaxed_dir: bool,
}

impl RuleMeta {
    /// Full applicability check: `is_rule_enabled` + directory-skip flags.
    ///
    /// This is the single seam the engine uses to decide whether to run a rule
    /// on a given file. All category/id knowledge lives in the rule's own META,
    /// not in the engine.
    pub fn applies_to(&self, file_ctx: &FileCtx, path: &Path, config: &Config) -> bool {
        config.is_rule_enabled(self.id, path) && self.applies_to_file(file_ctx)
    }

    /// Directory-skip check only (no `is_rule_enabled`).
    ///
    /// Use this only when `is_rule_enabled` has already been checked (e.g. the
    /// oxc fast path that pre-computes config-enabled flags once per run).
    pub fn applies_to_file(&self, file_ctx: &FileCtx) -> bool {
        !(self.skip_in_test_dir && file_ctx.path_segments.in_test_dir)
            && !(self.skip_in_relaxed_dir && file_ctx.path_segments.is_relaxed_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn make_meta(skip_in_test_dir: bool, skip_in_relaxed_dir: bool) -> RuleMeta {
        RuleMeta {
            id: "test-rule",
            description: "",
            remediation: "",
            severity: crate::diagnostic::Severity::Warning,
            doc_url: None,
            categories: &[],
            skip_in_test_dir,
            skip_in_relaxed_dir,
        }
    }

    fn make_file_ctx(in_test_dir: bool, is_relaxed_dir: bool) -> FileCtx {
        FileCtx {
            path_segments: PathSegments {
                in_test_dir,
                is_relaxed_dir,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn applies_to_file_table() {
        // (skip_in_test_dir, in_test_dir, skip_in_relaxed_dir, is_relaxed_dir) -> expected
        let cases = [
            (false, true, false, false, true),
            (true, false, false, false, true),
            (true, true, false, false, false),
            (false, false, true, true, false),
            (true, true, true, false, false),
        ];
        for (skip_test, in_test, skip_relaxed, in_relaxed, expected) in cases {
            let meta = make_meta(skip_test, skip_relaxed);
            let ctx = make_file_ctx(in_test, in_relaxed);
            assert_eq!(
                meta.applies_to_file(&ctx),
                expected,
                "skip_test={skip_test} in_test={in_test} skip_relaxed={skip_relaxed} in_relaxed={in_relaxed}"
            );
        }
    }
}
