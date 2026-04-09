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

use crate::diagnostic::Severity;

/// Stable identity + presentation for a lint rule.
///
/// The engine currently dispatches solely on the backends and uses the
/// backend-embedded `rule_id` string in each diagnostic. The RuleMeta is
/// carried alongside every rule so future features (JSON output with rule
/// metadata, `comply explain <rule>`, Oxlint diagnostic remapping) can
/// surface description/remediation/doc_url without re-plumbing.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Fields read by JSON output / explain / remap (coming soon).
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
}
