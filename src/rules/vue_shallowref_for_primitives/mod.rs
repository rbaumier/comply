//! vue-shallowref-for-primitives — prefer `shallowRef` for primitive values.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-shallowref-for-primitives",
    description: "`ref(<primitive>)` installs a deep reactive proxy — `shallowRef` is cheaper for primitives.",
    remediation: "Use `shallowRef(42)` / `shallowRef('x')` for primitives; primitives can't be deeply reactive anyway.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
