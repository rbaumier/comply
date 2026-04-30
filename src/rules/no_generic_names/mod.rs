//! no-generic-names — reject vague/meaningless identifier names along
//! two axes: exact banned words (`temp`, `result`, `val`, `foo`, …) and
//! banned prefixes (`process`, `data`, `do`, `execute`, `run`,
//! `perform`). `handle` is excluded because `handleXxx` is a React idiom.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names and mechanical prefixes carry no meaning.",
    remediation: "Rename to describe what the value IS or what the \
                  function accomplishes. `data` → `parsedOrder`, `temp` \
                  → name the actual intermediate, `processOrder` → \
                  `fulfillOrder`, `doPayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::disallowed_names")
}
