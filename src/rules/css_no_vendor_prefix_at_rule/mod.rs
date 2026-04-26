mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-no-vendor-prefix-at-rule",
    description: "Vendor-prefixed CSS at-rules are unnecessary with modern autoprefixer tooling.",
    remediation: "Remove the vendor prefix (e.g. `@-webkit-keyframes` → `@keyframes`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
