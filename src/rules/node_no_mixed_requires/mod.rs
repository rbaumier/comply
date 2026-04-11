//! node-no-mixed-requires

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-mixed-requires",
    description: "`require` calls should not be mixed with regular variable declarations.",
    remediation: "Separate `require()` declarations from non-require variable declarations.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-mixed-requires.md"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
