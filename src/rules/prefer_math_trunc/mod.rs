//! prefer-math-trunc — prefer `Math.trunc()` over bitwise truncation hacks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-math-trunc",
    description: "Prefer `Math.trunc(x)` over bitwise hacks like `x | 0`, `~~x`, or `x >> 0`.",
    remediation: "Replace bitwise truncation with `Math.trunc(x)`. Bitwise operators silently \
                  coerce to 32-bit integers and obscure intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
