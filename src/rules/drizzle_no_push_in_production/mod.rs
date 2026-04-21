//! drizzle-no-push-in-production — `drizzle-kit push` bypasses migrations.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-push-in-production",
    description: "`drizzle-kit push` is for dev only — use migrations in production/CI.",
    remediation: "Replace `drizzle-kit push` with `drizzle-kit generate` + `drizzle-kit migrate` in CI/deployment scripts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "drizzle"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
