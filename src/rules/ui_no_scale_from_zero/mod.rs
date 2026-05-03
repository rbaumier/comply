//! ui-no-scale-from-zero — flag inline `transform: scale(0)` styles.
//! Scaling from zero causes subpixel rendering blur during the animation
//! and makes elements appear from nowhere; prefer `scale(0.95)` paired
//! with `opacity: 0` for a natural entrance.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-scale-from-zero",
    description: "Inline `transform: scale(0)` causes subpixel rendering blur and makes elements \
                  appear from nowhere.",
    remediation: "Use `scale(0.95)` with `opacity: 0` for a smoother, more natural entrance.",
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
