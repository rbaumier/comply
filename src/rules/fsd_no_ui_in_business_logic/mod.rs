mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "fsd-no-ui-in-business-logic",
    description: "Feature-Sliced Design: business logic segments (model/api/lib) must not import from ui/.",
    remediation: "Don't import UI components from business logic layers",
    severity: Severity::Warning,
    doc_url: Some("https://feature-sliced.design/docs/reference/slices-segments"),
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
