//! rust-explicit-iter-loop — iterator chains over raw index loops.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-explicit-iter-loop",
    description: "Use iterator chains, not raw index loops.",
    remediation: "Replace `for i in 0..vec.len() { vec[i] }` with \
                  `for x in &vec`. Iterator chains let the compiler \
                  vectorize the loop body and eliminate bounds checks. \
                  Enable `clippy::needless_range_loop` and \
                  `clippy::explicit_iter_loop`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::Rust,
                Backend::Clippy {
                    lint: "clippy::explicit_iter_loop",
                },
            ),
            (
                Language::Rust,
                Backend::Clippy {
                    lint: "clippy::needless_range_loop",
                },
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Severity;
    use crate::rules::test_helpers::assert_clippy_rule;

    use super::*;

    #[test]
    fn registers_iter_loop_lints() {
        assert_clippy_rule(
            register(),
            "rust-explicit-iter-loop",
            Severity::Warning,
            &["clippy::explicit_iter_loop", "clippy::needless_range_loop"],
        );
    }

    #[test]
    fn metadata_mentions_iterator_replacement() {
        assert!(META.remediation.contains("for x in &vec"));
        assert_eq!(META.categories, &["rust"]);
    }
}
