//! explicit-units — numeric names with ambiguous bases need a unit suffix.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "explicit-units",
    description: "Numeric names should include an explicit unit (Ms, Bytes, Kb...).",
    remediation: "Add a unit suffix: `delay` → `delayMs`, `size` → \
                  `sizeBytes`, `rate` → `rateRps`. Ambiguous units cause \
                  real bugs — setTimeout(delay) expects ms.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
