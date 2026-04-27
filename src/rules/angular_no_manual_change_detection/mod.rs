//! angular-no-manual-change-detection — avoid `ChangeDetectorRef.detectChanges()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-manual-change-detection",
    description: "Manual change detection sidesteps OnPush / signals — usually a smell.",
    remediation: "Use signals or `ChangeDetectionStrategy.OnPush` with proper input mutations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
