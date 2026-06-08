//! ts-no-mixed-enums — enum with both numeric and string members.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-mixed-enums",
    description: "Enums mixing numeric and string members produce confusing inference and serialization.",
    remediation: "Pick one shape per enum — all string members, or all numeric. If the enum needs both kinds of values, split it into two enums.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-mixed-enums/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
