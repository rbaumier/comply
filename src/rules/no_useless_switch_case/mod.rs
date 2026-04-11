//! no-useless-switch-case

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-switch-case",
    description: "Disallow useless case in switch statements.",
    remediation: "Remove the empty case that falls through to `default` — \
                  it has no effect since `default` already handles all \
                  unmatched values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
