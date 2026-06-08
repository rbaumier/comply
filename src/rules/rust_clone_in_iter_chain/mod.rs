//! rust-clone-in-iter-chain — `.iter().map(|x| x.clone())` should be `.cloned()`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-clone-in-iter-chain",
    description: "`.map(|x| x.clone())` in an iterator chain — use `.cloned()`.",
    remediation: "`Iterator::cloned()` (or `.copied()` for `Copy` types) \
                  expresses intent more clearly and is the same in performance. \
                  The closure form makes readers ask whether anything else is \
                  going on inside the closure.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
