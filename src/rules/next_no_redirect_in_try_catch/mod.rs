//! next-no-redirect-in-try-catch — `redirect()` throws and must not be caught.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-redirect-in-try-catch",
    description: "`next/navigation` `redirect()` throws a control-flow error — catching it suppresses the redirect.",
    remediation: "Move `redirect()` outside the `try { ... }` block, or rethrow inside the catch \
                  using `if (isRedirectError(err)) throw err`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/redirect"),
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
