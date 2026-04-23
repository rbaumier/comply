//! Bans imports of legacy/heavy dependencies.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ban-dependencies",
    description: "Bans imports of legacy or heavy dependencies (lodash, moment, underscore).",
    remediation: "Use native alternatives or lighter libraries (date-fns, es-toolkit).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/nicolo-ribaudo/eslint-plugin-e18e"),
    categories: &["imports", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
