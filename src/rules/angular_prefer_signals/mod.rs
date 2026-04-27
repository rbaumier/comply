//! angular-prefer-signals — flag `BehaviorSubject` for component state.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-prefer-signals",
    description: "`new BehaviorSubject(...)` for component state — use `signal()` instead.",
    remediation: "Replace `BehaviorSubject` with `signal()` from `@angular/core`. Signals integrate with the change-detection runtime and template binding without manual subscriptions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
