//! no-this-in-static — disallow a bare `this` value and `super` in a static
//! context (member access through `this` stays valid for inherited statics).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-this-in-static",
    description: "A bare `this` value (or `super`) in a static context refers to the class, not an instance — usually a mistake.",
    remediation: "Use the class name for a bare `this` and the parent class name for `super`; member access through `this` (`this.x`, `new this()`) stays valid for inherited statics.",
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
