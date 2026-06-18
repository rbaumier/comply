//! ts-ban-tslint-comment — disallow `// tslint:<rule-flag>` comments.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-tslint-comment",
    description: "TSLint comments are obsolete — the project has been deprecated in favour of ESLint.",
    remediation: "Remove the `tslint:` comment directive.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-tslint-comment/"),
    categories: &["typescript"],

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
