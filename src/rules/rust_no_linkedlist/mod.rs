//! rust-no-linkedlist — use Vec<T>, not LinkedList<T>.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-linkedlist",
    description: "Prefer `Vec<T>` over `LinkedList<T>` — cache locality wins.",
    remediation: "Replace `LinkedList<T>` with `Vec<T>` or `VecDeque<T>`. \
                  LinkedList's theoretical O(1) splice is dominated in \
                  practice by Vec's cache locality for any realistic size. \
                  Enable `clippy::linkedlist`.",
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
                lint: "clippy::linkedlist",
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
    fn registers_linkedlist() {
        assert_clippy_rule(
            register(),
            "rust-no-linkedlist",
            Severity::Warning,
            &["clippy::linkedlist"],
        );
    }

    #[test]
    fn metadata_names_vec_replacements() {
        assert!(META.remediation.contains("Vec<T>"));
        assert!(META.remediation.contains("VecDeque<T>"));
    }
}
