//! React class components rely on lifecycle methods and instance state that
//! exist only in the client runtime. A class component in a server component
//! file can't render, so flag it at authoring time.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-class-component-in-server-component",
    description: "Class components don't render in server components.",
    remediation: "Rewrite as a function component, or move the class into a \
                  `\"use client\"` module.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components"),
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
