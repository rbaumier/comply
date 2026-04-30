//! ts-no-namespace — disallow TypeScript `namespace` keyword.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-namespace",
    description: "TypeScript `namespace` is a legacy construct — use ES modules instead.",
    remediation: "Replace the `namespace` with ES module exports (`export` / `import`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
