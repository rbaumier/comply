//! ts-no-export-equal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-export-equal",
    description: "CommonJS-style `export = ...` — prefer ES module exports.",
    remediation: "Use ES module exports: `export default` or named exports.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
