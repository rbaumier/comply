//! react-no-leaked-event-listener — `addEventListener` in `useEffect`
//! without a matching `removeEventListener` cleanup leaks across
//! re-renders and component unmounts.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-leaked-event-listener",
    description: "`addEventListener` in `useEffect` without a `removeEventListener` cleanup leaks across re-renders.",
    remediation: "Return a cleanup function from the effect that calls `removeEventListener` on the same target with the same listener and capture flag.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
