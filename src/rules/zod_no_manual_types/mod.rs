//! zod-no-manual-types — prefer `z.infer<typeof Schema>` over hand-rolled types
//! that mirror a Zod schema in the same file.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-manual-types",
    description: "A hand-written `type` that duplicates the keys of a nearby \
                  `z.object({...})` will drift from the schema and defeat runtime \
                  validation guarantees.",
    remediation: "Derive the type with `type T = z.infer<typeof Schema>` so the \
                  type always matches the schema.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

    skip_in_test_dir: false,
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
