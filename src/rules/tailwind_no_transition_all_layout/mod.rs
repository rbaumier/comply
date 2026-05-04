//! tailwind-no-transition-all-layout — forbid combining `transition-all`
//! with layout properties (width, height, top, left, right, bottom). These
//! trigger layout on every frame of the animation, causing jank. Prefer
//! `transition-transform` or `transition-opacity`, which the compositor can
//! handle without layout.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-transition-all-layout",
    description: "Forbid `transition-all` with width/height/top/left utilities — causes layout thrash.",
    remediation: "Animate via `translate-*` + `transition-transform` or `opacity-*` + `transition-opacity`. These are composited and never trigger layout.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "web-performance"],
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
