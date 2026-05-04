//! prisma-no-raw-query-interpolation — string-built `$queryRaw`/`$executeRaw`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-raw-query-interpolation",
    description: "`$queryRaw`/`$executeRaw` called with a non-tagged string concatenates input — SQL injection risk.",
    remediation: "Use `$queryRaw\\`SELECT ... ${value}\\`` (tagged template) or `$queryRawUnsafe(sql, ...params)` with placeholders.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["prisma", "security"],
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
