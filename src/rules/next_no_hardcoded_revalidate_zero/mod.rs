//! next-no-hardcoded-revalidate-zero

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-hardcoded-revalidate-zero",
    description: "`export const revalidate = 0` is a misleading way to opt out of caching.",
    remediation: "Use `export const dynamic = 'force-dynamic';` to express intent clearly.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/file-conventions/route-segment-config"),
    categories: &["nextjs"],
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
