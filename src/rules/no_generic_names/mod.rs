//! no-generic-names — reject standalone `data`/`info`/`temp`/`result`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names carry no meaning.",
    remediation: "Rename to describe what the value IS: `data` → \
                  `parsedOrder`, `info` → `userProfile`, `result` → \
                  `paymentReceipt`, `temp` → name the actual intermediate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::disallowed_names")
}
