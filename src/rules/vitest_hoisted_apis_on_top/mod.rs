//! vitest-hoisted-apis-on-top — `vi.mock` / `vi.hoisted` must appear before imports
//! (Vitest hoists them automatically, but placing them after imports hides that fact).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-hoisted-apis-on-top",
    description: "`vi.mock` / `vi.hoisted` are hoisted above imports — placing them after imports misleads readers.",
    remediation: "Move `vi.mock(...)` / `vi.hoisted(...)` calls to the top of the file, above all `import` statements.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/vitest-dev/eslint-plugin-vitest/blob/main/docs/rules/prefer-hoisted.md",
    ),
    categories: &["vitest"],
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
