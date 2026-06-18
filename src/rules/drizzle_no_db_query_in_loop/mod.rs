//! drizzle-no-db-query-in-loop

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-db-query-in-loop",
    description: "Drizzle queries inside `for` / `for-of` / `.map` / `.forEach` cause N+1 round-trips to the database.",
    remediation: "Hoist the query out of the loop and use `inArray(...)`/`leftJoin(...)`/`with: {...}` to fetch in a single round-trip.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "drizzle", "database"],

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
