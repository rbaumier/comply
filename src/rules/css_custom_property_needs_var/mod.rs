mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-custom-property-needs-var",
    description: "Custom properties must be referenced via `var()`.",
    remediation: "Wrap the `--name` in `var(--name)` to read its value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(css::Check)))],
    }
}
