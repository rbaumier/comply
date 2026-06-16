mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unknown-pseudo-class",
    description: "Disallow unknown CSS pseudo-class selectors.",
    remediation: "Use a known pseudo-class, a vendor-prefixed or custom (`--`) selector, or add the name to the rule's `ignore` list.",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/CSS/Pseudo-classes"),
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
