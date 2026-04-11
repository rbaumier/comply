//! prefer-native-coercion-functions — prefer passing `Number`, `String`, etc. directly.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-native-coercion-functions",
    description: "Prefer using `String`, `Number`, `BigInt`, `Boolean`, and `Symbol` directly.",
    remediation: "Pass the coercion function directly instead of wrapping it: \
                  `.map(Number)` instead of `.map(x => Number(x))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
