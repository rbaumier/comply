//! ts-ban-ts-comment — disallow `@ts-<directive>` comments or require descriptions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-ts-comment",
    description: "`@ts-ignore` and `@ts-nocheck` suppress compiler errors and hide bugs.",
    remediation: "Fix the underlying type error, or use `@ts-expect-error` with a description.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-ts-comment/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
