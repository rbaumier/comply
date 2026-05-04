//! zod-validate-env-at-startup — flag unvalidated `process.env.X` access in Zod projects.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-validate-env-at-startup",
    description: "`process.env.X` is read without an accompanying Zod \
                  validation of `process.env` in this file.",
    remediation: "Define a Zod schema for the required env vars and call \
                  `envSchema.parse(process.env)` (or `.safeParse`) once at \
                  startup. Export the parsed, typed object and consume it \
                  everywhere else instead of raw `process.env.X`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
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
