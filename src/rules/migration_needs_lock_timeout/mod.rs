//! migration-needs-lock-timeout

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-lock-timeout",
    description: "DDL migration without `SET lock_timeout` risks write queue pileups.",
    remediation: "Add `SET lock_timeout = '5s';` at the top of every DDL migration. Without it, an ALTER TABLE on a busy table queues all writes behind the lock indefinitely.",
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
