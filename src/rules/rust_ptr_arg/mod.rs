//! rust-ptr-arg — `&str`/`&[T]`/`&Path` over `&String`/`&Vec<T>`/`&PathBuf`.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-ptr-arg",
    description: "Prefer borrowed slices over borrowed owned types.",
    remediation: "Replace `&String` with `&str`, `&Vec<T>` with `&[T]`, \
                  `&PathBuf` with `&Path`. The slice form accepts more \
                  caller types with no extra cost. Enable `clippy::ptr_arg`.",
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
                lint: "clippy::ptr_arg",
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
    fn registers_ptr_arg() {
        assert_clippy_rule(
            register(),
            "rust-ptr-arg",
            Severity::Warning,
            &["clippy::ptr_arg"],
        );
    }

    #[test]
    fn metadata_names_slice_forms() {
        assert!(META.remediation.contains("&str"));
        assert!(META.remediation.contains("&[T]"));
        assert!(META.remediation.contains("&Path"));
    }
}
