//! zod-no-schema-in-hot-path — schemas must be defined at module scope.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-schema-in-hot-path",
    description: "Building a Zod schema inside a React render, a loop body, or a \
                  request handler allocates a new schema on every call — schemas \
                  are expensive to construct and should be cached.",
    remediation: "Hoist `z.object({...})` / `z.string()` to module scope and reference \
                  the same schema instance from your render / handler.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
