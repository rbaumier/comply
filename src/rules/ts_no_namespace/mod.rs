//! ts-no-namespace — disallow TypeScript `namespace` keyword.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
