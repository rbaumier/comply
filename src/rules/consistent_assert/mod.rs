//! consistent-assert

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-assert",
    description: "Prefer `assert.ok(…)` over bare `assert(…)` with `node:assert`.",
    remediation: "Replace bare `assert(…)` calls with `assert.ok(…)` for consistency with the `node:assert` API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
