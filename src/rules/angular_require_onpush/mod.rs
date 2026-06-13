//! angular-require-onpush — components should opt into OnPush change detection.

#[cfg(test)] mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-require-onpush",
    description: "Component lacks `changeDetection: ChangeDetectionStrategy.OnPush` — defaults to Default which re-checks every component on every event.",
    remediation: "Add `changeDetection: ChangeDetectionStrategy.OnPush` to the `@Component({...})` metadata.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular", "performance"],

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
