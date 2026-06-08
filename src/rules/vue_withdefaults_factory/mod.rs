//! vue-withdefaults-factory — array/object defaults in `withDefaults` must be factories.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-withdefaults-factory",
    description: "Array/object defaults in `withDefaults` must be factory functions — otherwise the default is shared.",
    remediation: "Use `items: () => []` / `config: () => ({})` — a literal `[]` or `{}` is the same object across instances.",
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
