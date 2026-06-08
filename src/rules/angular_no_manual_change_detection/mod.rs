//! angular-no-manual-change-detection — avoid `ChangeDetectorRef.detectChanges()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-manual-change-detection",
    description: "Manual change detection sidesteps OnPush / signals — usually a smell.",
    remediation: "Use signals or `ChangeDetectionStrategy.OnPush` with proper input mutations.",
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
