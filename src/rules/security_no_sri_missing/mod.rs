//! security-no-sri-missing

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-sri-missing",
    description: "`<script src=\"https://...\">` and `<link rel=\"stylesheet\">` from third-party origins must carry an `integrity` attribute (Subresource Integrity).",
    remediation: "Add an `integrity=\"sha384-…\"` attribute (and typically `crossOrigin=\"anonymous\"`) so the browser rejects tampered assets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
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
