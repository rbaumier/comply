//! elysia-t-unknown-format-string

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-t-unknown-format-string",
    description: "`t.String({ format: '...' })` accepts only a known set of format names — typos silently disable the format check.",
    remediation: "Use a recognised format (e.g. `email`, `uri`, `uuid`, `date`, `date-time`, `ipv4`, `ipv6`, `hostname`, `regex`, `time`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
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
