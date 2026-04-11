//! operation-returning-nan

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "operation-returning-nan",
    description: "Arithmetic operation will produce `NaN`.",
    remediation: "Convert the operand to a number first (`Number(x)`, `parseInt(x)`, `+x`) or fix the expression. Arithmetic on `undefined` or non-numeric strings always returns `NaN`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
