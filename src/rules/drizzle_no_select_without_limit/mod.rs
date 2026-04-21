mod text;
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
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
