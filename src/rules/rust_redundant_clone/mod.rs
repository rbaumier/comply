//! rust-redundant-clone — don't clone values that could be moved or borrowed.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-redundant-clone",
    description: "Remove `.clone()` calls whose result isn't independently observed.",
    remediation: "Move the value instead of cloning it, or borrow it if the \
                  caller still needs access. Clones allocate and copy — \
                  they're never free. Enable `clippy::redundant_clone`.",
    severity: Severity::Warning,
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
                lint: "clippy::redundant_clone",
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
    fn registers_redundant_clone() {
        assert_clippy_rule(
            register(),
            "rust-redundant-clone",
            Severity::Warning,
            &["clippy::redundant_clone"],
        );
    }

    #[test]
    fn metadata_says_move_or_borrow() {
        assert!(META.remediation.contains("Move the value"));
        assert!(META.remediation.contains("borrow it"));
    }
}
