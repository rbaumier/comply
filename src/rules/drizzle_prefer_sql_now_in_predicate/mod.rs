//! drizzle-prefer-sql-now-in-predicate

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-sql-now-in-predicate",
    description: "Flag a bare `new Date()` / `Date.now()` passed to a Drizzle filter operator (`eq`/`ne`/`gt`/`gte`/`lt`/`lte`/`between`).",
    remediation: "Compare against the database clock with `` sql`now()` `` (or `` sql`CURRENT_DATE` `` for date-only) instead of the app server's `new Date()` / `Date.now()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],

    skip_in_test_dir: true,
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
