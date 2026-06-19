//! tanstack-start-no-server-import-in-isomorphic-entry

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-server-import-in-isomorphic-entry",
    description: "Server-only packages (`@sentry/node`, `node:*`, `bun:*`, `pg`) statically imported in a TanStack Start isomorphic entry ship Node code into the client bundle.",
    remediation: "Move the import behind an `if (import.meta.env.SSR)`-gated dynamic `import()`, or relocate it to a server-only module.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
