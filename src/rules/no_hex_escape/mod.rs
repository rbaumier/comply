//! no-hex-escape

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-hex-escape",
    description: "Enforce the use of Unicode escapes instead of hexadecimal escapes.",
    remediation: "Replace `\\x41` with `\\u0041` — Unicode escapes are more consistent and readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
