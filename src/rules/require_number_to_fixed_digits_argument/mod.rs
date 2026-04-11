//! require-number-to-fixed-digits-argument

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "require-number-to-fixed-digits-argument",
    description: "Enforce using the digits argument with `Number#toFixed()`.",
    remediation: "Pass an explicit digits argument: `num.toFixed(0)`. The default is `0` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
