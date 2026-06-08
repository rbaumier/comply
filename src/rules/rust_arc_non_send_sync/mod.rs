//! rust-arc-non-send-sync — Arc around `!Send` or `!Sync` types is wrong.
//!
//! Doc-only marker rule. The actual enforcement lives in clippy:
//! `clippy::arc_with_non_send_sync` is in the `correctness` group
//! and warns by default. comply registers the rule so it shows up
//! in `comply list` / `comply explain` alongside the rest of the
//! Rust catalog, but does not run clippy itself.
//!
//! Example failure: `Arc<RefCell<T>>` — `RefCell` is `!Sync`, so the
//! `Arc` cannot be sent across threads. Either you don't need the
//! `Arc` (use plain `Rc<RefCell<T>>`) or you need a thread-safe
//! interior (`Arc<Mutex<T>>`).

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-arc-non-send-sync",
    description: "`Arc<T>` where `T: !Send + !Sync` cannot cross threads.",
    remediation: "Either drop the `Arc` (use `Rc<T>` for single-threaded \
                  sharing) or replace the inner type with a thread-safe \
                  one — `Arc<RefCell<T>>` → `Arc<Mutex<T>>`. Enforced by \
                  `clippy::arc_with_non_send_sync` (correctness, on by default).",
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
                lint: "clippy::arc_with_non_send_sync",
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
    fn registers_arc_with_non_send_sync() {
        assert_clippy_rule(
            register(),
            "rust-arc-non-send-sync",
            Severity::Error,
            &["clippy::arc_with_non_send_sync"],
        );
    }

    #[test]
    fn metadata_names_refcell_case() {
        assert!(META.remediation.contains("Arc<RefCell<T>>"));
        assert_eq!(META.categories, &["rust"]);
    }
}
