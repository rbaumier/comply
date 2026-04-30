//! no-test-return-statement — forbid `return` inside `test`/`it` callbacks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-test-return-statement",
    description: "Disallow `return` statements inside `test`/`it` callbacks.",
    remediation: "Remove return statement from test, use expect assertions",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
