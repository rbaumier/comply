//! react-iframe-missing-sandbox — `<iframe>` without `sandbox` attribute.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-iframe-missing-sandbox",
    description: "`<iframe>` without a `sandbox` attribute is a security risk.",
    remediation: "Add a `sandbox` attribute to the `<iframe>`. The `sandbox` \
                  attribute restricts the iframe's capabilities (scripts, forms, \
                  popups) and prevents it from accessing the parent page.",
    severity: Severity::Warning,
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
