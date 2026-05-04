//! tanstack-start-session-cookie-httponly — `useSession({ cookie })` must set `httpOnly: true`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-httponly",
    description: "`useSession({ cookie })` must set `httpOnly: true`.",
    remediation: "Add `httpOnly: true` to the cookie config so session cookies \
                  cannot be read from JavaScript (XSS mitigation).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
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
