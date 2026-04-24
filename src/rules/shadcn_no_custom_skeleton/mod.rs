//! shadcn-no-custom-skeleton — forbid hand-rolled skeletons built from
//! `<div className="animate-pulse …">`. Use the shadcn `<Skeleton>`
//! component instead.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-custom-skeleton",
    description: "Custom skeletons built from `animate-pulse` drift from the shadcn design tokens.",
    remediation: "Replace `<div className=\"animate-pulse …\">` with `<Skeleton className=\"…\" />`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
