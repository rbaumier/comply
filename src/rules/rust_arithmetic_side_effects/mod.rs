//! rust-arithmetic-side-effects — pick the overflow behaviour on purpose.
//!
//! Doc-only marker rule. Equivalent to
//! `clippy::arithmetic_side_effects` (restriction group, allow by
//! default — binding it here is what turns it on). `a + b` on integers
//! panics in a debug build and wraps in a release build, so the same
//! expression has two behaviours and neither was chosen: a length
//! parsed from a request, an offset read off the wire, a user-supplied
//! count all reach the arithmetic with values the author never picked.
//!
//! The lint fires on *every* arithmetic operation, including the ones
//! on values the crate fully controls. That breadth is the point — it
//! is what makes the untrusted inputs impossible to miss — but it is
//! also why a project that finds it too loud should turn the whole rule
//! off rather than sprinkle suppressions.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-arithmetic-side-effects",
    description: "Integer arithmetic that can overflow, wrap, or divide by zero.",
    remediation: "On integers that came from outside the process — a parsed \
                  argument, a request body, a wire format — say what should \
                  happen: `checked_add` when the caller has to handle it, \
                  `saturating_sub` when clamping is correct, `wrapping_mul` \
                  when wrap-around is the intent. `clippy::arithmetic_side_effects` \
                  fires on every arithmetic operation, values you control \
                  included; it is deliberately broad. If the noise outweighs \
                  the catch, turn it off wholesale with \
                  `[rules.rust-arithmetic-side-effects] disabled = true` in \
                  `comply.toml` rather than suppressing it line by line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "correctness"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::arithmetic_side_effects",
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
    fn registers_arithmetic_side_effects() {
        assert_clippy_rule(
            register(),
            "rust-arithmetic-side-effects",
            Severity::Error,
            &["clippy::arithmetic_side_effects"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_the_opt_out() {
        assert!(META.remediation.contains("clippy::arithmetic_side_effects"));
        assert!(META.remediation.contains("checked_add"));
        assert!(
            META.remediation
                .contains("[rules.rust-arithmetic-side-effects] disabled = true")
        );
        assert_eq!(META.categories, &["rust", "correctness"]);
    }
}
