//! no-console-spaces

//! no-console-spaces

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-console-spaces",
    description: "Leading/trailing spaces in `console.log` arguments produce misaligned output.",
    remediation: "Remove the leading or trailing space from the string argument. Use comma-separated arguments for spacing instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
