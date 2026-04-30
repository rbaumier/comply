//! angular-require-onpush — components should opt into OnPush change detection.

mod typescript;

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
