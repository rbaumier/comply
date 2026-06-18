//! ts-no-dupe-class-members — disallow duplicate class members.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dupe-class-members",
    description: "Duplicate class members shadow earlier definitions and indicate a bug.",
    remediation: "Remove or rename the duplicate class member. TS method overloads (without a body) are allowed.",
    severity: Severity::Error,
    doc_url: Some("https://typescript-eslint.io/rules/no-dupe-class-members"),
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
