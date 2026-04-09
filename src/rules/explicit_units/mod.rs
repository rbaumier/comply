//! explicit-units — numeric names with ambiguous bases need a unit suffix.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "explicit-units",
    description: "Numeric names should include an explicit unit (Ms, Bytes, Kb...).",
    remediation: "Add a unit suffix: `delay` → `delayMs`, `size` → \
                  `sizeBytes`, `rate` → `rateRps`. Ambiguous units cause \
                  real bugs — setTimeout(delay) expects ms.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
