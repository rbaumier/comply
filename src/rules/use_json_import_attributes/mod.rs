//! use-json-import-attributes — require `with { type: "json" }` on JSON imports.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-json-import-attributes",
    description: "A default import of a `.json` module is missing the `type: \"json\"` import attribute.",
    remediation: "Add `with { type: \"json\" }` to the import so the runtime parses the module as JSON.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-json-import-attributes/"),
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
