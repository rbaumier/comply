//! use-single-jsdoc-asterisk — ported from Biome's `useSingleJsDocAsterisk`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-single-jsdoc-asterisk",
    description: "JSDoc comment lines should start (and end before `*/`) with a single asterisk.",
    remediation: "Remove the extra asterisk so the line begins with one `*` after the indentation.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-single-js-doc-asterisk/"),
    categories: &["jsdoc"],

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
