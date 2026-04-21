mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-returning-on-insert-update",
    description: "Drizzle insert/update without `.returning()` wastes a round-trip on a follow-up SELECT.",
    remediation: "Chain `.returning()` to get the inserted/updated row in a single query.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/insert#insert-returning"),
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
