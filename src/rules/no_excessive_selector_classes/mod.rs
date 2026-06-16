mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-excessive-selector-classes",
    description: "Limit the number of class selectors in a single CSS selector.",
    remediation: "Reduce the number of chained class selectors, or split the selector into simpler selectors.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-excessive-selector-classes/"),
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
