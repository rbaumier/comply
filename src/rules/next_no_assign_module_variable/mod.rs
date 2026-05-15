//! next-no-assign-module-variable — overwriting `module` breaks Next.js page builds.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-assign-module-variable",
    description: "Assigning to `module` (e.g. `let module = ...`) shadows Node's module object and breaks Next.js page builds.",
    remediation: "Rename the local variable. Reserved names: `module`, `exports`, `require`, `process`, `Buffer`, `__dirname`, `__filename`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-assign-module-variable"),
    categories: &["next"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
