//! next-no-html-link-for-pages

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-html-link-for-pages",
    description: "Using `<a href=\"/internal-route\">` causes a full page reload.",
    remediation: "Use `<Link>` from `next/link` for internal navigation to keep client-side routing.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-html-link-for-pages"),
    categories: &["nextjs", "performance"],
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
