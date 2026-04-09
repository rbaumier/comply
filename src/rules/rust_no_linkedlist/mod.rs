//! rust-no-linkedlist — use Vec<T>, not LinkedList<T>.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-linkedlist",
    description: "Prefer `Vec<T>` over `LinkedList<T>` — cache locality wins.",
    remediation: "Replace `LinkedList<T>` with `Vec<T>` or `VecDeque<T>`. \
                  LinkedList's theoretical O(1) splice is dominated in \
                  practice by Vec's cache locality for any realistic size. \
                  Enable `clippy::linkedlist`.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy { lint: "clippy::linkedlist" },
        )],
    }
}
