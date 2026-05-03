//! next-no-client-import-in-server

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-client-import-in-server",
    description: "Browser-only modules cannot be imported into server components.",
    remediation: "Move the import into a `\"use client\"` boundary, or replace it with a server-safe alternative.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/rendering/composition-patterns"),
    categories: &["nextjs", "rsc"],
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
