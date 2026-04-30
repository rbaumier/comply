//! tanstack-query-no-set-query-data-without-key-factory — flag
//! `setQueryData([...inline...])` calls. Inline keys make cache writes
//! impossible to find / refactor; use a query key factory.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-set-query-data-without-key-factory",
    description: "`setQueryData` with an inline array key is invisible to refactors.",
    remediation: "Use a query key factory: `setQueryData(userKeys.detail(id), data)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
