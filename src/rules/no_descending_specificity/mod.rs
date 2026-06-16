mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-descending-specificity",
    description: "Disallow a lower specificity selector from coming after a higher specificity selector.",
    remediation: "Reorder the rules so the higher-specificity selector comes after the one it overrides.",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/CSS/Specificity"),
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
