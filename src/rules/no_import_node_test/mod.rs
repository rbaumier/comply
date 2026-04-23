//! no-import-node-test

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-import-node-test",
    description: "Importing from `node:test` alongside vitest/jest mixes test runners.",
    remediation: "Don't mix node:test with vitest/jest",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
