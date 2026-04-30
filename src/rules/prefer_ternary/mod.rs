//! prefer-ternary — flag `if (c) { x = a; } else { x = b; }` -> ternary.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-ternary",
    description: "Simple if/else assignment can be a ternary expression.",
    remediation: "Replace `if (c) { x = a; } else { x = b; }` with \
                  `x = c ? a : b;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
