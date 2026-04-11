//! node-no-process-env

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-process-env",
    description: "Direct use of `process.env` is discouraged.",
    remediation: "Centralize environment access in a config module instead of scattering `process.env` reads.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-process-env.md"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
