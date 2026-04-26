//! rust-large-enum-variant — box large variants so the enum stays small.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-large-enum-variant",
    description: "Enum size equals the largest variant — box big variants.",
    remediation: "Wrap the large variant's payload in `Box<T>` so the enum \
                  stays small. Otherwise every instance of the enum — even \
                  the small-variant case — pays the full size cost. Enable \
                  `clippy::large_enum_variant`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::large_enum_variant",
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
    fn registers_large_enum_variant() {
        assert_clippy_rule(
            register(),
            "rust-large-enum-variant",
            Severity::Warning,
            &["clippy::large_enum_variant"],
        );
    }

    #[test]
    fn metadata_mentions_boxed_payload() {
        assert!(META.remediation.contains("Box<T>"));
        assert_eq!(META.categories, &["rust"]);
    }
}
