//! i18n-key-requires-domain-prefix — flag t() keys missing a `domain.subkey`
//! prefix so locale files stay organised.
//!
//! Skipped in test directories (`skip_in_test_dir`): flat single-segment keys
//! there are intentional fixtures (e.g. an i18n library exercising its own
//! locale-fallback/lookup engine with minimal message catalogs), not real
//! application i18n usage.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-key-requires-domain-prefix",
    description: "t() key is missing a domain prefix (`domain.key`).",
    remediation: "Namespace every key under a domain so locale files stay organised: `auth.login.title`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],

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
