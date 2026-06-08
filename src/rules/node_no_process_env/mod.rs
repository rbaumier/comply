//! node-no-process-env

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-process-env",
    description: "Direct use of `process.env` is discouraged.",
    remediation: "Centralize environment access in a config module instead of scattering `process.env` reads.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-process-env.md",
    ),
    categories: &["node"],

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
