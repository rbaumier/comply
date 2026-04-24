//! ui-no-keyframes-for-interruptible — state-driven (class-toggled)
//! animations should use `transition`, not `@keyframes`, so they can
//! interrupt gracefully.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-keyframes-for-interruptible",
    description: "Class-toggled (state-driven) animations should use `transition`; `@keyframes` can't interrupt mid-flight.",
    remediation: "Replace the `@keyframes` + `animation:` pair with `transition` on the same properties so toggles interpolate from the current value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
