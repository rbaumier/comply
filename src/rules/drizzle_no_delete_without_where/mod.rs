//! drizzle-no-delete-without-where — bulk `.delete()` calls without filter.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-delete-without-where",
    description: "`.delete(table)` without `.where(...)` purges every row in the table.",
    remediation: "Chain `.where(...)` to scope the delete. If you really want to truncate, use `db.execute(sql\\`TRUNCATE …\\`)` with an explicit comment.",
    severity: Severity::Error,
    doc_url: Some("https://orm.drizzle.team/docs/eslint-plugin#drizzlerecommended"),
    categories: &["drizzle", "safety"],
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
