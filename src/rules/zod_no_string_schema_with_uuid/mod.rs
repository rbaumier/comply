//! zod-no-string-schema-with-uuid — prefer `z.uuid()` over `z.string().uuid()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-string-schema-with-uuid",
    description: "`z.string().uuid()` is deprecated in Zod v4 — use the top-level `z.uuid()` schema.",
    remediation: "Use z.uuid() instead of z.string().uuid() in Zod v4+",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

    // In test files, `z.string().uuid()` is either the correct v3 API (no
    // top-level `z.uuid()` exists there) or a deliberate backward-compat test of
    // the deprecated chained form — not a v4 migration the author should make.
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
