//! vue-button-has-type

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-button-has-type",
    description: "`<button>` without an explicit `type` attribute defaults to `submit`, which may cause unexpected form submissions.",
    remediation: "Add an explicit `type` attribute (`button`, `submit`, or `reset`) to every `<button>` element.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue", "html"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
