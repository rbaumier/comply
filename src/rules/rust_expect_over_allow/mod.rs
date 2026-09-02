//! rust-expect-over-allow — `#[expect]` expires, `#[allow]` does not.
//!
//! Doc-only marker rule. Equivalent to `clippy::allow_attributes`
//! (restriction group, allow by default — binding it here is what turns
//! it on). `#[expect(lint, reason = "…")]` is an assertion that the lint
//! currently fires: once the code stops warning, the attribute itself
//! becomes an `unfulfilled_lint_expectations` warning and the build
//! points at the now-pointless suppression. `#[allow]` makes no such
//! claim — it survives the refactor that removed the problem and keeps
//! the lint silent for whatever lands there next.
//!
//! Sibling rule: `rust-no-allow-without-reason` requires a suppression
//! to carry a justification. This one requires the self-expiring form,
//! whether or not a reason is attached.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-expect-over-allow",
    description: "Suppress lints with `#[expect(…)]`, not `#[allow(…)]`.",
    remediation: "Replace `#[allow(lint)]` with \
                  `#[expect(lint, reason = \"…\")]`. `#[expect]` fails the \
                  build the day the warning it suppresses disappears, so \
                  the suppression dies with the problem; an `#[allow]` \
                  outlives it and silently covers the next offender that \
                  lands under it. Keep `#[allow]` only where the lint \
                  fires conditionally (a `cfg`-gated build, a generated \
                  file) and `#[expect]` would itself warn. \
                  Enforced by `clippy::allow_attributes`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::allow_attributes",
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
    fn registers_allow_attributes() {
        assert_clippy_rule(
            register(),
            "rust-expect-over-allow",
            Severity::Error,
            &["clippy::allow_attributes"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_the_expect_form() {
        assert!(META.remediation.contains("clippy::allow_attributes"));
        assert!(META.remediation.contains("#[expect(lint, reason ="));
        assert_eq!(META.categories, &["rust"]);
    }
}
