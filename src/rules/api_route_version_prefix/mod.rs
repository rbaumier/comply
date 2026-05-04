mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-route-version-prefix",
    description: "API routes must start with a version prefix (/v1/, /v2/, …).",
    remediation: "Prefix the route path with a version segment, e.g. `/v1/users`. If routes are mounted on a versioned sub-router, disable this rule for that file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
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
