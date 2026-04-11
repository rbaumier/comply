//! no-useless-fallback-in-spread

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-fallback-in-spread",
    description: "Disallow useless fallback when spreading in object literals.",
    remediation: "Remove the `|| {}` or `?? {}` fallback — spreading \
                  `undefined`/`null` is already a no-op in object literals.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
