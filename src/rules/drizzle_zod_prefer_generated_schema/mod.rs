mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-zod-prefer-generated-schema",
    description: "Manual `z.object({})` in a Drizzle schema file duplicates column definitions.",
    remediation: "Use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` to generate Zod schemas from the table definition.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/zod"),
    categories: &["drizzle", "zod"],
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
