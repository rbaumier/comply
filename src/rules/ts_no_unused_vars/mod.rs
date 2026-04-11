//! ts-no-unused-vars — flag declared variables that are never referenced.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-vars",
    description: "Declared variables that are never used are dead code.",
    remediation: "Remove the unused variable or prefix with `_` to indicate intentional non-use.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-vars"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
