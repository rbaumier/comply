//! drizzle-no-update-without-where — bulk `.update()` calls without filter.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-update-without-where",
    description: "`.update(table).set(...)` without `.where(...)` overwrites every row in the table.",
    remediation: "Chain `.where(...)` after `.set(...)` to scope the update. If a bulk update is intended, add a comment so the next reviewer doesn't think it's a bug.",
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
