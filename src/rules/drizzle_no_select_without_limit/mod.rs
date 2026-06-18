//! drizzle-no-select-without-limit

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-select-without-limit",
    description: "`db.select().from(table)` without `.limit()` or `.where()` scans the entire table.",
    remediation: "Add `.limit(n)` or `.where(condition)` to bound the result set.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/select#basic-and-partial-select"),
    categories: &["drizzle", "database"],

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
