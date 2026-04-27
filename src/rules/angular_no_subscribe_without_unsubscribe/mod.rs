//! angular-no-subscribe-without-unsubscribe — leak-prone subscription patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-subscribe-without-unsubscribe",
    description: "`.subscribe()` without `takeUntil`/`takeUntilDestroyed`/`DestroyRef` leaks subscriptions.",
    remediation: "Use `takeUntilDestroyed()` (Angular 16+), the `async` pipe, or unsubscribe in `ngOnDestroy`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
