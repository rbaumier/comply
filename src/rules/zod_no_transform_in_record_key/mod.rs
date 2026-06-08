//! zod-no-transform-in-record-key — using `.transform()` in a `z.record()`
//! key schema produces runtime keys Zod cannot round-trip: the transformed
//! output is used as the object key but the original input is gone, so
//! validation asymmetries and parse/serialize mismatches follow.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-transform-in-record-key",
    description: "`.transform()` inside a `z.record()` key schema mutates the object key after validation, causing parse/serialize asymmetry.",
    remediation: "Don't use transforms in record key schemas",
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
