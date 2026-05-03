//! Detects potential XPath injection vulnerabilities.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
