//! next-no-redirect-in-try-catch — `redirect()` throws and must not be caught.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-redirect-in-try-catch",
    description: "`next/navigation` `redirect()` throws a control-flow error — catching it suppresses the redirect.",
    remediation: "Move `redirect()` outside the `try { ... }` block, or rethrow inside the catch \
                  using `if (isRedirectError(err)) throw err`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/redirect"),
    categories: &["nextjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
