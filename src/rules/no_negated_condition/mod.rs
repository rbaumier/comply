//! no-negated-condition — flag `if (!x) { A } else { B }` and `!x ? A : B`.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-negated-condition",
    description: "Disallow negated conditions with an else branch.",
    remediation: "Swap the if/else branches (or ternary arms) and remove the negation \
                  for clearer intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
