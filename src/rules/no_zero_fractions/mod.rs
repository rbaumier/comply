//! no-zero-fractions

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-zero-fractions",
    description: "Disallow number literals with zero fractions or dangling dots.",
    remediation: "Remove the unnecessary `.0` fraction — write `1` instead of `1.0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
