//! security-cookie-no-samesite-none — `SameSite=None` requires `Secure`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-cookie-no-samesite-none",
    description: "`SameSite=None` cookies must also set `Secure` — browsers reject them otherwise, and they're cleartext-leakable.",
    remediation: "Pair `sameSite: 'none'` with `secure: true` (or write `SameSite=None; Secure` \
                  on the raw header).",
    severity: Severity::Error,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie#samesitesamesite-value",
    ),
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
