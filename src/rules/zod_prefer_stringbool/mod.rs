//! zod-prefer-stringbool — prefer `z.stringbool()` over `z.coerce.boolean()` (Zod v4).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-stringbool",
    description: "`z.coerce.boolean()` only checks truthiness — any non-empty string \
                  (including `\"false\"`) becomes `true`, which breaks HTML form inputs \
                  and query strings.",
    remediation: "Use `z.stringbool()` (Zod v4) to parse `\"true\"/\"false\"/\"1\"/\"0\"` \
                  robustly, or write an explicit `.transform()` with allowed values.",
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
