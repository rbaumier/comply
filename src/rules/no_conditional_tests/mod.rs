//! no-conditional-tests

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-conditional-tests",
    description: "`describe`/`test`/`it` calls must not be wrapped in conditional control flow.",
    remediation: "Don't conditionally define tests, use test.skip or describe.skip",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
