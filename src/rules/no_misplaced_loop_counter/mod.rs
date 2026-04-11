//! no-misplaced-loop-counter

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-misplaced-loop-counter",
    description: "`for` loop update clause modifies a different variable than the condition.",
    remediation: "Ensure the update expression (`i++`) modifies the same variable used in the loop condition (`i < n`). Mismatched variables usually indicate a copy-paste bug.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
