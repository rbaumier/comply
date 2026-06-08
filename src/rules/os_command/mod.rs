//! Detects potential OS command injection vulnerabilities.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "os-command",
    description: "Detects potential OS command injection via exec/spawn with dynamic input.",
    remediation: "Use `execFile`/`spawnSync` with separate arguments array, never interpolate user input into shell commands.",
    severity: Severity::Error,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-2076"),
    categories: &["security", "sonarjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
