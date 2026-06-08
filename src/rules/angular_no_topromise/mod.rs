//! angular-no-topromise — flag deprecated `.toPromise()` calls.

#[cfg(test)] mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-topromise",
    description: "`.toPromise()` is deprecated since RxJS 7 and removed in v8 — converts a subscription incorrectly when the source emits no value.",
    remediation: "Use `firstValueFrom(observable$)` (or `lastValueFrom`) from `rxjs`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular", "rxjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
