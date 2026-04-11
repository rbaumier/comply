//! react-jsx-no-script-url — no `javascript:` URLs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-script-url",
    description: "`href=\"javascript:...\"` is an XSS vector.",
    remediation: "Use an `onClick` handler instead of a `javascript:` URL. \
                  Script URLs bypass CSP and enable cross-site scripting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
