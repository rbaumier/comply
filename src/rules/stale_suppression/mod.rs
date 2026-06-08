//! stale-suppression — flag `// comply-ignore` markers that suppress nothing.
//!
//! When a developer adds a suppression to silence a warning, but later the
//! code changes and the rule no longer fires on the target line, the marker
//! becomes dead code papering over a problem that no longer exists. Worse,
//! the next reader assumes the suppression is load-bearing and won't touch it.
//!
//! Detection is post-processing in `crate::ignore_comments::apply_suppressions`,
//! not a tree-sitter check — we need the full diagnostic list (including
//! delegated oxlint/clippy output) to decide whether a marker matched
//! anything. This module exists only so the rule has a `RuleMeta` entry in
//! `meta_registry`, which feeds the catalog, the pretty renderer's
//! help/url section, and `comply explain stale-suppression`.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "stale-suppression",
    description: "A `// comply-ignore` comment that no longer suppresses any diagnostic — \
                  the rule it silences doesn't fire on the target line.",
    remediation: "Delete the suppression comment. If the underlying violation has come \
                  back, the rule will re-fire on its own and you can decide whether to \
                  re-add the suppression with a fresh justification.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments", "suppressions"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// Register the rule with no backends — diagnostics are emitted by
/// `crate::ignore_comments::apply_suppressions` after all other passes run.
/// The empty backend list is intentional: `engine::collect_applicable`
/// short-circuits rules whose `(language, backend)` set doesn't match the
/// current file, so this entry is effectively meta-only.
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
