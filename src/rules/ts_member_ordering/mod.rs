//! ts-member-ordering — require a consistent order for class/interface members.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-member-ordering",
    description: "Class and interface members should follow a consistent order: signatures, fields, constructors, methods.",
    remediation: "Re-order members: put signatures first, then fields, then constructors, then methods.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/member-ordering"),
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
