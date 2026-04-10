//! banned-identifiers — rename any identifier starting with `process` /
//! `handle` / `data` / `do` / `execute` / `run` / `perform` on a word
//! boundary. These verbs describe mechanics, not intent.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "banned-identifiers",
    description: "Banned prefixes describe mechanics, not intent.",
    remediation: "Rename to express what this accomplishes, not how. \
                  `processOrder` → `fulfillOrder`, `handlePayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::disallowed_names")
}
