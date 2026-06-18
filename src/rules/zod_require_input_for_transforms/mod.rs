//! zod-require-input-for-transforms — prefer `z.input` when deriving a type
//! from a schema that applies `.transform()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-input-for-transforms",
    description: "`z.infer<typeof Schema>` returns the *output* type of a schema. \
                  For schemas that use `.transform()`, the input shape (what the user \
                  actually types into a form) differs from the output.",
    remediation: "Use `z.input<typeof Schema>` for form values and `z.output<typeof Schema>` \
                  (or `z.infer`) for the parsed result.",
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
