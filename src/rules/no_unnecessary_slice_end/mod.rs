//! no-unnecessary-slice-end

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-slice-end",
    description: "Disallow unnecessary `.length` or `Infinity` as the `end` argument of `slice()`.",
    remediation: "Remove the second argument: `.slice(start)` already goes to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
