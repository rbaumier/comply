//! JSX event handlers (`onClick`, `onChange`, `onSubmit`, …) need the React
//! client runtime. Inside a server component they're inert at best and cause
//! hydration-time errors at worst. Flag them at authoring time.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-event-handler-in-server-component",
    description: "Event handlers (`onClick`, `onChange`, …) can't run in a server component.",
    remediation: "Move interactive JSX into a client component (`\"use client\"`), \
                  or use a server action via `<form action={...}>` for form submits.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components#serializable-props"),
    categories: &["react"],

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
