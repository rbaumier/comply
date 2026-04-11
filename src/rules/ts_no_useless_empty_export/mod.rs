//! ts-no-useless-empty-export — flag `export {}` when other exports exist.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-useless-empty-export",
    description: "`export {}` is unnecessary when the file already has other exports.",
    remediation: "Remove the `export {}` statement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
