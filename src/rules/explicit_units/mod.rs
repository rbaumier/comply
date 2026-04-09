//! explicit-units — numeric names with ambiguous bases need a unit suffix.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
