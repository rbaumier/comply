//! hono-jwt-secret-hardcoded — JWT secret must not be a string literal.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-jwt-secret-hardcoded",
    description: "`jwt({ secret: \"...\" })` uses a hardcoded secret — anyone with the source can sign tokens.",
    remediation: "Read the secret from an environment variable (`secret: env.JWT_SECRET`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["hono", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
