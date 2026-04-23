//! Requires explicit target origin in postMessage calls.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "post-message-origin",
    description: "Requires explicit target origin in `postMessage()` calls.",
    remediation: "Specify a target origin instead of `'*'`: `postMessage(data, 'https://example.com')`.",
    severity: Severity::Error,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-2819"),
    categories: &["security", "sonarjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
