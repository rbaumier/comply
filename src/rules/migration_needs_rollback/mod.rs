//! migration-needs-rollback

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-rollback",
    description: "Migration without a `down`/rollback function is irreversible.",
    remediation: "Add an explicit `down()` / `rollback()` function to every migration. Irreversible migrations prevent quick recovery from bad deploys. Make data migrations idempotent with `ON CONFLICT DO NOTHING`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
