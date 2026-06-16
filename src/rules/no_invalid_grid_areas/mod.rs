mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-grid-areas",
    description: "Disallow invalid named grid areas in CSS Grid Layouts.",
    remediation: "Give every grid-area string the same number of cell tokens and make each named area a single filled-in rectangle.",
    severity: Severity::Error,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/CSS/grid-template-areas"),
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
