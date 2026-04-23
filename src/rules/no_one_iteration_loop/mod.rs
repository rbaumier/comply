//! no-one-iteration-loop

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-one-iteration-loop",
    description: "Loop body always exits on the first iteration.",
    remediation: "Remove the loop — it is equivalent to the body running once. If a loop is intended, ensure the exit statement is guarded by a condition.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S1751"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
