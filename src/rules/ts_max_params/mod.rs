//! ts-max-params — enforce a maximum number of parameters in function definitions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-max-params",
    description: "Functions with too many parameters are hard to understand and maintain.",
    remediation: "Reduce the number of parameters by using an options object or refactoring.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/max-params"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
