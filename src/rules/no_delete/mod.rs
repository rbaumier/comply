//! no-delete

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-delete",
    description: "Disallow `delete` except on a local copy the function solely owns — removing a key mutates the object in place.",
    remediation: "Build a new object without the property, e.g. `const { [key]: _, ...rest } = obj;` or use `Object.fromEntries(Object.entries(obj).filter(...))`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["functional"],

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
