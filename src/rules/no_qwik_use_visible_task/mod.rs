//! no-qwik-use-visible-task — flag `useVisibleTask$()` in Qwik components.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-qwik-use-visible-task",
    description: "`useVisibleTask$()` runs eagerly on mount, blocking hydration and hurting Qwik's resumability.",
    remediation: "Prefer `useTask$()`/`useResource$()`, or pass `{ strategy: 'document-idle' }` when visible-task is required.",
    severity: Severity::Error,
    doc_url: Some("https://biomejs.dev/linter/rules/no-qwik-use-visible-task/"),
    categories: &["correctness", "qwik"],

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
