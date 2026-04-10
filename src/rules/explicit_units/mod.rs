//! explicit-units — numeric names with ambiguous bases need a unit suffix.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
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
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
