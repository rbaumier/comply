//! tanstack-start-session-cookie-secure — `useSession({ cookie })` must set `secure`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-secure",
    description: "`useSession({ cookie })` must set `secure`.",
    remediation: "Add `secure: true` (or `secure: process.env.NODE_ENV === 'production'`) \
                  so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],

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
