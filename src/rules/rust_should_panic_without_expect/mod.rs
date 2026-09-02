//! rust-should-panic-without-expect — `#[should_panic]` with no `expected`.
//!
//! Doc-only marker rule. Equivalent to
//! `clippy::should_panic_without_expect` (pedantic group, allow by
//! default — binding it here is what turns it on). A bare
//! `#[should_panic]` accepts *any* panic: an `unwrap()` in the fixture,
//! an out-of-bounds index in the setup, a `todo!()` still sitting in
//! the code under test. The test stays green while proving nothing
//! about the failure it was written for. comply registers the rule for
//! documentation parity with the rest of the Rust catalog but defers
//! enforcement to clippy.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-should-panic-without-expect",
    description: "`#[should_panic]` without `expected = \"…\"` passes on any panic.",
    remediation: "Name the failure the test is about: \
                  `#[should_panic(expected = \"divide by zero\")]`. Without \
                  the `expected` fragment the test also goes green when the \
                  fixture's own `unwrap()` blows up or a `todo!()` fires \
                  first, so it stops proving anything about the code under \
                  test. Enforced by `clippy::should_panic_without_expect`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::should_panic_without_expect",
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
    fn registers_should_panic_without_expect() {
        assert_clippy_rule(
            register(),
            "rust-should-panic-without-expect",
            Severity::Error,
            &["clippy::should_panic_without_expect"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_the_expected_form() {
        assert!(
            META.remediation
                .contains("clippy::should_panic_without_expect")
        );
        assert!(META.remediation.contains("#[should_panic(expected ="));
        assert_eq!(META.categories, &["rust", "testing"]);
    }
}
