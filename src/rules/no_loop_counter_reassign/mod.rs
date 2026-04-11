//! no-loop-counter-reassign

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-loop-counter-reassign",
    description: "Assignment to a `for` loop counter inside the loop body causes subtle bugs.",
    remediation: "Use a separate variable instead of reassigning the loop counter. Modifying the counter inside the body makes the loop hard to reason about and often hides off-by-one errors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
