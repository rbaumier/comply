//! ui-no-large-animated-blur — flag inline `filter: blur(Npx)` styles where
//! the blur radius exceeds 20px. Large blur radii are expensive (cost grows
//! with radius and layer size) and can exhaust GPU memory on mobile.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-large-animated-blur",
    description: "Inline `filter: blur(Npx)` with N > 20 — expensive, escalates with radius and \
                  layer size, can exhaust GPU memory on mobile.",
    remediation: "Reduce the blur radius below 20px, or composite the blur statically into a \
                  background image.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
