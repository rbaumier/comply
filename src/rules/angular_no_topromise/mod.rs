//! angular-no-topromise — flag deprecated `.toPromise()` calls.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-topromise",
    description: "`.toPromise()` is deprecated since RxJS 7 and removed in v8 — converts a subscription incorrectly when the source emits no value.",
    remediation: "Use `firstValueFrom(observable$)` (or `lastValueFrom`) from `rxjs`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular", "rxjs"],
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
