//! banned-identifiers — rename any identifier starting with `process` /
//! `data` / `do` / `execute` / `run` / `perform` on a word boundary.
//! These verbs describe mechanics, not intent. `handle` is intentionally
//! excluded because `handleXxx` is the canonical React event-handler
//! naming convention (`onClick={handleClick}`).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "banned-identifiers",
    description: "Banned prefixes describe mechanics, not intent.",
    remediation: "Rename to express what this accomplishes, not how. \
                  `processOrder` → `fulfillOrder`, `doPayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};
pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::disallowed_names")
}
