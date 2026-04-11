//! prefer-export-from — use `export { x } from` for re-exports.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-export-from",
    description: "Prefer `export { x } from './m'` over import-then-re-export.",
    remediation: "Replace `import { x } from './m'; export { x };` with \
                  `export { x } from './m';`. Direct re-export is shorter, \
                  avoids a binding in the local scope, and makes the re-export \
                  intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
