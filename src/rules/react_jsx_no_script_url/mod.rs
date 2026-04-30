//! react-jsx-no-script-url — no `javascript:` URLs.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef {
        meta: META,
        backends,
    }
}
