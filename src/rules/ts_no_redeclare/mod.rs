//! ts-no-redeclare — disallow variable redeclaration in the same scope.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-redeclare",
    description: "Redeclaring a variable in the same scope shadows the previous declaration silently.",
    remediation: "Remove the duplicate declaration or rename the variable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-redeclare"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
