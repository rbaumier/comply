//! react-no-async-useeffect-callback — `useEffect` expects a sync callback
//! whose return value is the cleanup function. Passing `async` returns a
//! promise instead, so React calls `.then(promise)` as a cleanup — almost
//! never what you want.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-async-useeffect-callback",
    description: "`useEffect` callback must be sync — async callbacks return a promise, breaking cleanup.",
    remediation: "Define an async function inside the effect and call it: \
                  `useEffect(() => { (async () => { await fetch(); })(); }, [...])`. \
                  Or use a library that supports async (e.g. SWR, React Query).",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/react/useEffect#fetching-data-with-effects"),
    categories: &["react"],
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
