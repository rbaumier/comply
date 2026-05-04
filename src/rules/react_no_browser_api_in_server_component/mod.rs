//! Browser globals (`window`, `document`, `localStorage`, …) are undefined
//! during server render. Touching them in a server component crashes the
//! request at render time — flag early.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-browser-api-in-server-component",
    description: "Browser globals (`window`, `document`, `localStorage`) don't exist on the server.",
    remediation: "Move the browser-only code into a `\"use client\"` component, \
                  gate it behind `useEffect`, or use a server-safe alternative.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components"),
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
