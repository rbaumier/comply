//! jsdoc/check-values — imported from eslint-plugin-jsdoc.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/check-values",
    description: "`@version`, `@since`, `@license` must have a valid value.",
    remediation: "Give `@version` / `@since` a semver-ish string (e.g. `1.2.3`). Give `@license` a non-empty SPDX identifier (e.g. `MIT`). Remove the tag if you don't have a value.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-values.md",
    ),
    categories: &["jsdoc"],
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
