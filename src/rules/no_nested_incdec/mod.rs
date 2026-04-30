//! no-nested-incdec

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-incdec",
    description: "`++` or `--` used inside an expression, not as a standalone statement.",
    remediation: "Separate the increment/decrement from the expression. Write `i++; arr[i] = x;` instead of `arr[i++] = x;` to make the order of operations explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
