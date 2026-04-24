mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-await-inside-gen",
    description: "In Result.gen, Promise-returning Results must use `yield* Result.await(...)`.",
    remediation: "Replace `await` with `yield* Result.await(...)` inside Result.gen generators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
