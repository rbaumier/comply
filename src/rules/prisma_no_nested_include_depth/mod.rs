//! prisma-no-nested-include-depth — flag `include` chains nested deeper than 3 levels.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-nested-include-depth",
    description: "Deeply nested `include:` (>3 levels) creates huge join queries that are slow and hard to reason about.",
    remediation: "Split the query into multiple targeted reads, or denormalise the columns you actually need with `select`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "performance"],
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
