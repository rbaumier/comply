//! no-multi-op-oneliner — reject dense chained operators on a single line.

mod dense_lines;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-multi-op-oneliner",
    description: "Dense one-liners with many chained operators resist review.",
    remediation: "Extract intermediate named variables. Each step of the \
                  expression should have a name that says what it represents \
                  — `activeItems`, `prices`, `subtotal`, `total`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
