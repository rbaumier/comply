mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "fsd-no-global-store-imports",
    description: "Lower FSD layers (entities/shared/widgets) must not import the global store directly.",
    remediation: "Don't import global store from lower layers, use dependency injection",
    severity: Severity::Warning,
    doc_url: Some("https://feature-sliced.design/docs/reference/layers"),
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
