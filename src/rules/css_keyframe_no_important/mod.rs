mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-keyframe-no-important",
    description: "Disallow `!important` inside `@keyframes` declarations.",
    remediation: "Remove `!important` — browsers ignore it inside keyframes.",
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
