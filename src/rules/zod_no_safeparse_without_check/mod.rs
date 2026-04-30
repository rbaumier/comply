//! zod-no-safeparse-without-check — flag `.safeParse(x).data` without a
//! `.success` check. `safeParse` returns a discriminated union; reading
//! `.data` directly silently ignores validation failures.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-safeparse-without-check",
    description: "Reading `.data` from `safeParse` without checking `.success` ignores validation errors.",
    remediation: "Branch on `result.success` first, or use `.parse()` and let it throw.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["zod", "typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
