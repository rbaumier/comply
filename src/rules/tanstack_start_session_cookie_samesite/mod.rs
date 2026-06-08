//! tanstack-start-session-cookie-samesite — `useSession({ cookie })` must set
//! `sameSite` to `'lax'` or `'strict'`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-samesite",
    description: "`useSession({ cookie })` must set `sameSite` to `'lax'` or `'strict'`.",
    remediation: "Add `sameSite: 'lax'` (default) or `sameSite: 'strict'` to the \
                  cookie config to mitigate CSRF.",
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
