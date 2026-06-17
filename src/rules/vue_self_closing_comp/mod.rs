//! vue-self-closing-comp

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-self-closing-comp",
    description: "Components without children should use self-closing syntax in Vue templates.",
    remediation: "Replace `<Foo></Foo>` with `<Foo />`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
