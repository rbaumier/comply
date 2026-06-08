//! ts-no-import-type-side-effects — enforce top-level `import type` when
//! all specifiers use inline `type` qualifiers.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-import-type-side-effects",
    description: "Inline `type` qualifiers on every specifier leave a side-effect import at runtime.",
    remediation: "Use a top-level `import type { ... }` instead of `import { type A, type B }`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-import-type-side-effects/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
