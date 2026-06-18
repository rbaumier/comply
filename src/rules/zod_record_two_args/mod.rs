//! zod-record-two-args — require `z.record(keySchema, valueSchema)` (Zod v4).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-record-two-args",
    description: "`z.record(valueSchema)` is removed in Zod v4 — the single-arg \
                  form leaves the key type implicit and makes migrations painful.",
    remediation: "Pass both arguments explicitly: `z.record(z.string(), valueSchema)`. \
                  Use a branded or enum key schema when you want a narrower key type.",
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
