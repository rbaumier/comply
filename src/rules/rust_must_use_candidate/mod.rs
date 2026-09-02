//! rust-must-use-candidate — a pure public function's result is the point.
//!
//! Doc-only marker rule. Equivalent to `clippy::must_use_candidate`
//! (pedantic group, allow by default — binding it here is what turns it
//! on). When a public function mutates nothing and hands back a value,
//! calling it and dropping the result is dead code by construction:
//! `s.trim();` on its own line does nothing at all. `#[must_use]` turns
//! that from a silent no-op into a compiler warning at every call site,
//! including the ones outside this crate.
//!
//! Sibling rule: `rust-builder-without-must-use` puts `#[must_use]` on
//! builder *types* so a dropped chain warns. This one covers the
//! functions.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-must-use-candidate",
    description: "Public side-effect-free functions returning a value need `#[must_use]`.",
    remediation: "Add `#[must_use]` above the function. A `pub fn` that \
                  mutates nothing and returns a value exists only for its \
                  result, so ignoring it is always the caller's bug — \
                  `#[must_use]` makes the compiler say so instead of \
                  compiling a no-op. Builder types are already covered by \
                  `rust-builder-without-must-use`; this is the function-level \
                  half. Enforced by `clippy::must_use_candidate`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "api"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::must_use_candidate",
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
    fn registers_must_use_candidate() {
        assert_clippy_rule(
            register(),
            "rust-must-use-candidate",
            Severity::Error,
            &["clippy::must_use_candidate"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_defers_builders() {
        assert!(META.remediation.contains("clippy::must_use_candidate"));
        assert!(META.remediation.contains("rust-builder-without-must-use"));
        assert_eq!(META.categories, &["rust", "api"]);
    }
}
