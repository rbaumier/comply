//! vue-no-value-on-reactive — forbid `.value` on a variable produced by `reactive()`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-value-on-reactive",
    description: "`.value` on a `reactive()` variable is undefined — only refs need `.value`.",
    remediation: "Remove `.value` — reactive proxies expose their keys directly: `state.count`, not `state.value.count`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
