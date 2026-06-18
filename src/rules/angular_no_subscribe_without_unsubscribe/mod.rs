//! angular-no-subscribe-without-unsubscribe — leak-prone subscription patterns.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-subscribe-without-unsubscribe",
    description: "`.subscribe()` without `takeUntil`/`takeUntilDestroyed`/`DestroyRef` leaks subscriptions.",
    remediation: "Use `takeUntilDestroyed()` (Angular 16+), the `async` pipe, or unsubscribe in `ngOnDestroy`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],

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
