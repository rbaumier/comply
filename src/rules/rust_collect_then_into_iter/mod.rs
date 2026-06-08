//! rust-collect-then-into-iter — `.collect::<Vec<_>>().into_iter()` round-trip.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-collect-then-into-iter",
    description: "`.collect::<Vec<_>>().into_iter()` materialises and \
                  immediately re-iterates a collection.",
    remediation: "Drop the `.collect()` + `.into_iter()` pair — the \
                  preceding iterator chain already produces an iterator. \
                  Materialising into `Vec` only to re-iterate allocates \
                  a heap buffer for nothing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
