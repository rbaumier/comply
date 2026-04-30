//! nestjs-no-entity-in-controller — controllers must not import ORM entities.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-entity-in-controller",
    description: "Controllers should not import ORM entities — leak persistence into the HTTP layer.",
    remediation: "Return a DTO mapped from the entity inside the service, never expose the entity \
                  directly from the controller.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
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
