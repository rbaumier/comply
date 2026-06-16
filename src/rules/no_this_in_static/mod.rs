//! no-this-in-static — disallow `this` and `super` in a static context.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-this-in-static",
    description: "`this` and `super` in a static context refer to the class, not an instance — usually a mistake.",
    remediation: "Replace `this` with the class name and `super` with the parent class name.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-this-in-static/"),
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
