//! ts-prefer-namespace-keyword — require `namespace` over `module` keyword.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-namespace-keyword",
    description: "Use `namespace` instead of `module` to declare custom TypeScript modules.",
    remediation: "Replace the `module` keyword with `namespace`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-namespace-keyword"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
