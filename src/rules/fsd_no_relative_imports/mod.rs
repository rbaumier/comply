mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "fsd-no-relative-imports",
    description: "Feature-Sliced Design: relative imports must not traverse across slices or layers.",
    remediation: "Use absolute imports or shared layer for cross-slice dependencies",
    severity: Severity::Warning,
    doc_url: Some("https://feature-sliced.design/docs/reference/layers"),
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
