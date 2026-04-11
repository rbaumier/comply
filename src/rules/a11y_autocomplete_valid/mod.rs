//! a11y-autocomplete-valid

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-autocomplete-valid",
    description: "The `autoComplete` attribute must use a valid value.",
    remediation: "Use a valid autocomplete token such as `name`, `email`, `username`, `new-password`, etc. See the HTML spec for the full list.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
