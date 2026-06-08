//! vue-use-template-ref — prefer `useTemplateRef` over `ref(null)` for template refs (Vue 3.5+).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-use-template-ref",
    description: "`ref(null)` as a template ref is superseded by `useTemplateRef('name')` in Vue 3.5+.",
    remediation: "Replace `const el = ref(null)` + `ref=\"el\"` with `const el = useTemplateRef('el')`.",
    severity: Severity::Warning,
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
