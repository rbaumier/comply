mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-satisfies",
    description: "`as Type` on object/array literal widens the type — use `satisfies` instead.",
    remediation: "Replace `{...} as Type` with `{...} satisfies Type`. `satisfies` validates the literal without losing the narrow inferred type.",
    severity: Severity::Warning,
    doc_url: Some("https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-9.html"),
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
