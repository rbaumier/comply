//! rust-doc-sections-required — what the signature cannot say, the rustdoc says.
//!
//! Doc-only marker rule. Equivalent to `clippy::missing_panics_doc` +
//! `clippy::missing_safety_doc`. A caller reads the signature and the doc,
//! never the body: a function that can panic looks total until it aborts the
//! process, and a `pub unsafe fn` is uncallable without knowing the invariants
//! it assumes. Neither shows up in the signature, so the `# Panics` and
//! `# Safety` sections are where that contract lives.
//!
//! Failures do show up in the signature — `-> Result<T, E>` names the error
//! type — so no `# Errors` section is required, and
//! [`rust-no-errors-doc-section`][crate::rules::rust_no_errors_doc_section]
//! flags the ones that get written anyway.
//!
//! `missing_safety_doc` warns by default; `missing_panics_doc` is pedantic and
//! allows by default — binding it here is what turns it on.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-doc-sections-required",
    description: "Public rustdoc carries `# Panics` and `# Safety`.",
    remediation: "Add the section the signature cannot carry: `# Panics` on a \
                  public `fn` that can panic (which inputs make it), \
                  `# Safety` on a `pub unsafe fn` (the invariants the caller \
                  must uphold). The caller reads the doc, not the body — if \
                  the section is hard to write, the panic is probably worth \
                  removing instead. Enforced by `clippy::missing_panics_doc` \
                  and `clippy::missing_safety_doc`.",
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
    fn registers_the_two_doc_lints() {
        assert_clippy_rule(
            register(),
            "rust-doc-sections-required",
            Severity::Error,
            &["clippy::missing_panics_doc", "clippy::missing_safety_doc"],
        );
    }

    #[test]
    fn does_not_require_an_errors_section() {
        // `clippy::missing_errors_doc` produced a `# Errors` section that
        // paraphrases the signature; `rust-no-errors-doc-section` now flags
        // those instead, so this rule must not demand one back.
        assert!(!META.remediation.contains("# Errors"));
        for (_language, backend) in register().backends {
            assert!(!format!("{backend:?}").contains("missing_errors_doc"));
        }
    }

    #[test]
    fn remediation_names_the_two_lints_and_the_sections() {
        assert!(META.remediation.contains("clippy::missing_panics_doc"));
        assert!(META.remediation.contains("clippy::missing_safety_doc"));
        assert!(META.remediation.contains("# Panics"));
        assert!(META.remediation.contains("# Safety"));
        assert_eq!(META.categories, &["rust"]);
    }
}
