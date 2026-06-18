//! zod-no-coerce-on-financial — forbid `z.coerce.*` on money/price/amount/currency fields.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-coerce-on-financial",
    description: "`z.coerce.number()` silently accepts `\"NaN\"`, `\" 1.2 \"`, and \
                  empty strings — catastrophic for money/price/amount/currency fields.",
    remediation: "Parse the input explicitly: `z.string().regex(/^\\d+(\\.\\d{1,2})?$/)\
                  .transform(Number)`, and reject anything else with a clear error.",
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
