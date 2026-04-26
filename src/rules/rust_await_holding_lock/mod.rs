//! rust-await-holding-lock — never hold a lock across .await.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-await-holding-lock",
    description: "Never hold a MutexGuard across an `.await` point.",
    remediation: "Drop the guard before awaiting: copy the needed data out \
                  in a tight scope, `drop(guard)`, then await. Locks held \
                  across awaits cause deadlocks under tokio's scheduler. \
                  Enable `clippy::await_holding_lock`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::await_holding_lock",
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
    fn registers_await_holding_lock() {
        assert_clippy_rule(
            register(),
            "rust-await-holding-lock",
            Severity::Error,
            &["clippy::await_holding_lock"],
        );
    }

    #[test]
    fn metadata_requires_drop_before_await() {
        assert!(META.remediation.contains("Drop the guard before awaiting"));
        assert_eq!(META.categories, &["rust"]);
    }
}
