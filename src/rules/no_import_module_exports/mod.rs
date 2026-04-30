//! no-import-module-exports

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-import-module-exports",
    description: "File mixes `import` declarations with `module.exports`.",
    remediation: "Use either ES module syntax (`import`/`export`) or CommonJS (`require`/`module.exports`), not both.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
