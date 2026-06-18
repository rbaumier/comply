//! next-no-server-import-in-client

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-server-import-in-client",
    description: "Server-only modules (`fs`, `net`, `next/server`, `server-only`) cannot run in the browser.",
    remediation: "Move the import to a server module, or remove the `\"use client\"` directive.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/rendering/composition-patterns"),
    categories: &["nextjs", "rsc"],

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
