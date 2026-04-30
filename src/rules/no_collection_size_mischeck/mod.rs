//! no-collection-size-mischeck

//! no-collection-size-mischeck

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-collection-size-mischeck",
    description: "`.length >= 0` is always true; `.length < 0` is always false.",
    remediation: "Use `.length > 0` to check non-empty, or `.length === 0` to check empty.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
