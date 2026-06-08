//! zod-require-schema-suffix — exported Zod schemas should end in `Schema`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-schema-suffix",
    description: "Exported Zod schemas should be named with a `Schema` suffix.",
    remediation: "Rename the export so the name ends in `Schema` (e.g. \
                  `export const UserSchema = z.object({...})`). The naming \
                  convention keeps the schema distinguishable from the \
                  inferred TypeScript type (`type User = z.infer<typeof UserSchema>`).",
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
