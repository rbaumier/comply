//! zod-prefer-error-over-message — Zod v4 renamed the `message` param to `error`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-error-over-message",
    description: "Zod v4 renamed the `message` error-customization param to `error`. A string \
                  `message` key in a `z.*` call still works but is the deprecated v3 spelling.",
    remediation: "Rename the `message` key to `error` in the Zod call, e.g. \
                  `z.string({ error: '...' })` or `.min(1, { error: '...' })`.",
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
