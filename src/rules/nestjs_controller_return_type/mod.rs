//! nestjs-controller-return-type — controllers must declare explicit return types.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-controller-return-type",
    description: "Controller methods should declare an explicit return type annotation.",
    remediation: "Annotate the method with `: Promise<Dto>` / `: Observable<Dto>` so the response \
                  contract is part of the type system, not inferred per-implementation.",
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
