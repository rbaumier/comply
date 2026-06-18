//! drizzle-relations-missing-inverse

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-relations-missing-inverse",
    description: "A `relations(...)` block declares a `one(...)` / `many(...)` reference whose inverse isn't defined in the same file.",
    remediation: "Add the inverse `relations(...)` for the referenced table so Drizzle's relational query API resolves both directions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle"],

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
