//! zod-no-unknown-schema — `z.unknown()` opts out of validation.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-unknown-schema",
    description: "`z.unknown()` accepts anything — the schema provides no validation.",
    remediation: "Replace `z.unknown()` with a concrete schema that describes \
                  the expected shape (e.g. `z.object({...})`, `z.string()`, \
                  `z.array(...)`). If the value truly is unknown until runtime, \
                  validate it at the boundary where the shape becomes known.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

    // In test files, `z.unknown()` is the schema type under test (a validation
    // library exercising its own `unknown` behavior) or a harmless fixture — not
    // a shipped-schema oversight the author should replace with a concrete type.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
