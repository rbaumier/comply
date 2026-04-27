//! react-no-router-push-in-effect-no-dep — `router.push(...)` inside
//! `useEffect(..., [])` always navigates on first render. Almost always
//! a bug — should be conditional or in an event handler.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-router-push-in-effect-no-dep",
    description: "`router.push(...)` in a mount-only `useEffect` always navigates — almost certainly a bug.",
    remediation: "Move the navigation into an event handler, gate it on a condition the effect depends on, \
                  or perform the redirect server-side (e.g. `redirect()` in Next.js).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "nextjs"],
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
