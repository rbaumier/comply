//! angular-no-lifecycle-in-service — services don't have component lifecycle hooks.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-lifecycle-in-service",
    description: "Component lifecycle hooks like `ngOnInit` are never invoked on `@Injectable()` services.",
    remediation: "Move initialization to the constructor or use `OnDestroy` only with `providedIn: 'root'`.",
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
