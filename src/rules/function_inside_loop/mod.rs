//! Detects function declarations inside loops — creates new function object each iteration.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "function-inside-loop",
    description: "Function declared inside loop creates new function object each iteration.",
    remediation: "Move the function outside the loop, or use a method reference.",
    severity: Severity::Warning,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-1515"),
    categories: &["sonarjs", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
