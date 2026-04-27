//! angular-no-direct-dom — direct DOM access bypasses Angular's rendering pipeline.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-direct-dom",
    description: "Direct DOM access bypasses Angular's rendering pipeline.",
    remediation: "Use `Renderer2` or `@ViewChild` instead of `document.getElementById`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
