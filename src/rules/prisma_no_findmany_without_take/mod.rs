//! prisma-no-findmany-without-take — bound result sets to avoid OOM on large tables.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-findmany-without-take",
    description: "`findMany()` without `take` returns the entire table — risk of OOM and slow responses.",
    remediation: "Add a `take: N` (or `first: N` for the legacy v1 API). For pagination, combine `take` + `skip` or cursor pagination.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "performance", "safety"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
