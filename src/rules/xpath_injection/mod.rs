//! Detects potential XPath injection vulnerabilities.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xpath-injection",
    description: "Detects potential XPath injection via dynamic query strings.",
    remediation: "Use parameterized XPath queries or escape user input.",
    severity: Severity::Error,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-2091"),
    categories: &["security", "sonarjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
