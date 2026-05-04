//! `server-only` is a runtime poison pill — importing it from a client
//! component bundle throws at module evaluation. If the file is marked
//! `"use client"`, any `server-only` import is a contradiction.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-server-only-in-client",
    description: "`server-only` can't be imported from a client component.",
    remediation: "Remove `\"use client\"` from this file, or remove the \
                  `server-only` import and move server-side logic to a \
                  separate server module.",
    severity: Severity::Error,
    doc_url: Some("https://www.npmjs.com/package/server-only"),
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
