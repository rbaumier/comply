//! rust-trivially-copy-pass-by-ref — small `Copy` types belong in registers.
//!
//! Doc-only marker rule. Equivalent to
//! `clippy::trivially_copy_pass_by_ref` (pedantic group, allow by
//! default — binding it here is what turns it on). A `Copy` type small
//! enough to travel in a register is cheaper to hand over by value than
//! behind a pointer: `&Span` costs a load plus a dereference on every
//! field read, and the indirection keeps the optimizer from holding the
//! value in a register.
//!
//! Two limits come from clippy's own configuration, which comply does
//! not override: the size cut-off is `trivial-copy-size-limit`, 8 bytes
//! by default, and `avoid-breaking-exported-api` — on by default —
//! keeps the lint off a crate's public signatures, so it fires on
//! internal helpers first.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-trivially-copy-pass-by-ref",
    description: "Pass a small `Copy` type by value, not by reference.",
    remediation: "Drop the `&`: `fn at(span: Span)` instead of \
                  `fn at(span: &Span)`. A `Copy` type that fits in a \
                  register travels for free, while `&Span` costs a load \
                  plus a dereference on every field read — pointer chasing \
                  for a value that was already cheap to copy. Enforced by \
                  `clippy::trivially_copy_pass_by_ref`, whose cut-off is \
                  clippy's `trivial-copy-size-limit` (8 bytes by default).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::trivially_copy_pass_by_ref",
            },
        )],
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Severity;
    use crate::rules::test_helpers::assert_clippy_rule;

    use super::*;

    #[test]
    fn registers_trivially_copy_pass_by_ref() {
        assert_clippy_rule(
            register(),
            "rust-trivially-copy-pass-by-ref",
            Severity::Error,
            &["clippy::trivially_copy_pass_by_ref"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_the_by_value_form() {
        assert!(
            META.remediation
                .contains("clippy::trivially_copy_pass_by_ref")
        );
        assert!(META.remediation.contains("fn at(span: Span)"));
        assert_eq!(META.categories, &["rust", "performance"]);
    }
}
