//! no-await-expression-member — flag `(await expr).prop` / `(await expr)[0]`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-await-expression-member",
    description: "Do not access a member directly from an await expression.",
    remediation: "Extract the awaited value into a variable, then access the member: \
                  `const response = await fetch(url); const data = response.json();`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],

    // Accessing a member on an awaited value (`(await api.get(url)).data`) is the
    // idiomatic, intentionally-concise pattern in HTTP integration tests using
    // supertest/axios; the readability smell this rule guards against is a
    // production-code concern, so test files are exempt.
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
