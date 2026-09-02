//! rust-doc-sections-required — the failure modes belong in the rustdoc.
//!
//! Doc-only marker rule. Equivalent to `clippy::missing_errors_doc` +
//! `clippy::missing_panics_doc` + `clippy::missing_safety_doc`. A caller
//! reads the signature and the doc, never the body: `-> Result<T, E>`
//! says a failure exists but not which ones, a function that can panic
//! looks total until it aborts the process, and a `pub unsafe fn` is
//! uncallable without knowing the invariants it assumes. The three
//! sections are where that contract lives.
//!
//! `missing_safety_doc` warns by default; the other two are pedantic
//! and allow by default — binding them here is what turns them on.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-doc-sections-required",
    description: "Public rustdoc carries `# Errors`, `# Panics` and `# Safety`.",
    remediation: "Add the section the signature implies: `# Errors` on a \
                  public `fn` returning `Result` (which conditions produce \
                  which `Err`), `# Panics` on one that can panic (which \
                  inputs make it), `# Safety` on a `pub unsafe fn` (the \
                  invariants the caller must uphold). The caller reads the \
                  doc, not the body — if the section is hard to write, the \
                  failure mode is probably worth removing instead. Enforced \
                  by `clippy::missing_errors_doc`, \
                  `clippy::missing_panics_doc` and \
                  `clippy::missing_safety_doc`.",
    severity: Severity::Error,
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
                    lint: "clippy::missing_errors_doc",
                },
            ),
            (
                Language::Rust,
                Backend::Clippy {
                    lint: "clippy::missing_panics_doc",
                },
            ),
            (
                Language::Rust,
                Backend::Clippy {
                    lint: "clippy::missing_safety_doc",
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
    fn registers_the_three_doc_lints() {
        assert_clippy_rule(
            register(),
            "rust-doc-sections-required",
            Severity::Error,
            &[
                "clippy::missing_errors_doc",
                "clippy::missing_panics_doc",
                "clippy::missing_safety_doc",
            ],
        );
    }

    #[test]
    fn remediation_names_the_three_lints_and_the_sections() {
        assert!(META.remediation.contains("clippy::missing_errors_doc"));
        assert!(META.remediation.contains("clippy::missing_panics_doc"));
        assert!(META.remediation.contains("clippy::missing_safety_doc"));
        assert!(META.remediation.contains("# Errors"));
        assert!(META.remediation.contains("# Panics"));
        assert!(META.remediation.contains("# Safety"));
        assert_eq!(META.categories, &["rust"]);
    }
}
