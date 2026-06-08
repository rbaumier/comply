//! import-no-amd

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-amd",
    description: "AMD `require` and `define` calls are forbidden.",
    remediation: "Use ES module `import` instead of AMD `require([...], fn)` or `define([...], fn)`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-amd.md",
    ),
    categories: &["imports"],

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
