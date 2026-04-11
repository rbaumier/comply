//! prefer-module — prefer ESM over CommonJS.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-module",
    description: "Prefer ESM (`import`/`export`) over CommonJS (`require`/`module.exports`).",
    remediation: "Replace `require()` with `import`, `module.exports` / \
                  `exports.x` with `export`, and `__dirname` / `__filename` \
                  with `import.meta.dirname` / `import.meta.filename`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
