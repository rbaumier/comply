//! tanstack-start-no-client-import-in-server-fn

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-client-import-in-server-fn",
    description: "Client-only React imports in a `.functions.ts` or `.server.ts` file — server functions cannot use browser APIs.",
    remediation: "Move client-only logic out of server-function files. Only import server-safe deps.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tanstack", "react"],

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
