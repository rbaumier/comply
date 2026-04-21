//! tanstack-start-no-client-import-in-server-fn

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-client-import-in-server-fn",
    description: "Client-only React imports in a `.functions.ts` file — server functions cannot use browser APIs.",
    remediation: "Move client-only logic out of `.functions.ts`. Only import server-safe deps.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tanstack", "react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
