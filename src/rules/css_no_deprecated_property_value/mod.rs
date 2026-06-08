mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-no-deprecated-property-value",
    description: "Disallow deprecated keyword values in CSS declarations.",
    remediation: "Replace the deprecated keyword with the modern equivalent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(css::Check)))],
    }
}
