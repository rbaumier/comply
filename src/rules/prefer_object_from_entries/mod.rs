//! prefer-object-from-entries

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-from-entries",
    description: "Prefer `Object.fromEntries()` over building objects from key-value pairs via `reduce`.",
    remediation: "Use `Object.fromEntries(arr.map(…))` instead of `arr.reduce((acc, …) => ({ ...acc, … }), {})`. It is more readable and avoids quadratic spread copies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
