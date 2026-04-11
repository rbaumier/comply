//! react-no-danger-with-children — dangerouslySetInnerHTML + children.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-danger-with-children",
    description: "Using both `dangerouslySetInnerHTML` and `children` on the same element is invalid.",
    remediation: "Use either `dangerouslySetInnerHTML` OR `children`, not both. \
                  React will throw a runtime error when both are provided on \
                  the same element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
