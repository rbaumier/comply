//! next-no-api-route-in-middleware

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-api-route-in-middleware",
    description: "Calling your own API route from middleware causes a same-origin fetch loop.",
    remediation: "Inline the logic, call a shared helper, or invoke a third-party endpoint — never fetch your own `/api/*` from middleware.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/routing/middleware"),
    categories: &["nextjs", "reliability"],
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
