mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "fsd-no-cross-slice-dependency",
    description: "Feature-Sliced Design: slices at the same layer must not import from each other.",
    remediation: "Don't import directly between slices at same layer. Use shared layer or public API.",
    severity: Severity::Warning,
    doc_url: Some("https://feature-sliced.design/docs/reference/layers"),
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
