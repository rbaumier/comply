//! React hooks can't run inside a server component — they need a client
//! runtime. Flagging early means developers see the violation in their editor
//! instead of at render time.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-client-hook-in-server-component",
    description: "React hooks can only run in client components.",
    remediation: "Add `\"use client\"` at the top of the file, or move the hook \
                  call into a separate client component and import it.",
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
