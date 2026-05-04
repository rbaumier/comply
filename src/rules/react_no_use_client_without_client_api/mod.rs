//! react-no-use-client-without-client-api — `"use client"` in a file with no client-only APIs.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-use-client-without-client-api",
    description: "`\"use client\"` directive in a file that uses no hooks, event handlers, or browser APIs.",
    remediation: "Remove the `\"use client\"` directive so the module can render on the server, \
                  or add the client-only behavior that justifies it.",
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
