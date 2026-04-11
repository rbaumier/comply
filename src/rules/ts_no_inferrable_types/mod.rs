//! ts-no-inferrable-types — flag explicit types when TS can infer them.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-inferrable-types",
    description: "Explicit types on variables initialized with literals are redundant — TypeScript infers them.",
    remediation: "Remove the type annotation and let TypeScript infer the type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
