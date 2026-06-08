//! vue-void-elements-no-children

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-void-elements-no-children",
    description: "Void HTML elements (`<br>`, `<img>`, `<input>`) cannot have children.",
    remediation: "Remove children from void elements.",
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
