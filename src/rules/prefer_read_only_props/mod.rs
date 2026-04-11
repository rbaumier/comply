//! prefer-read-only-props

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-read-only-props",
    description: "React component props should be wrapped in `Readonly<>`.",
    remediation: "Wrap the props type: `(props: Readonly<MyType>)` or `({ x }: Readonly<MyType>)`. This prevents accidental mutation of props.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
