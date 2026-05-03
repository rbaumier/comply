//! ui-no-scroll-trigger-markers-prod — GSAP `ScrollTrigger` `markers: true`
//! must be guarded by `process.env.NODE_ENV` to avoid shipping debug UI.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-scroll-trigger-markers-prod",
    description: "`markers: true` in a ScrollTrigger config ships debug overlays to production.",
    remediation: "Gate it: `markers: process.env.NODE_ENV !== \"production\"`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
