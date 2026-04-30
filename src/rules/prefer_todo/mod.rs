//! prefer-todo — flag empty test bodies and suggest `test.todo`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-todo",
    description: "Empty test body — use `test.todo` to mark unimplemented tests.",
    remediation: "Use test.todo('description') for placeholder tests",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
