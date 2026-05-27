//! no-typeof-prefer-schema

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-prefer-schema",
    description: "Validating an object's shape with chained `typeof` checks is \
                  error-prone — use a schema validator (zod, valibot, …).",
    remediation: "Replace the chained `typeof` checks with a schema parsed at the \
                  boundary, e.g. `const User = z.object({ name: z.string(), age: \
                  z.number() }); User.parse(data)`. A single `typeof x === 'string'` \
                  narrowing is fine — this targets multi-property shape checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
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
