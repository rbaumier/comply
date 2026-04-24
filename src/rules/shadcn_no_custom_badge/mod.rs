//! shadcn-no-custom-badge — forbid badge-looking `<span>` built from
//! `rounded-full bg-*` utilities. Use the shadcn `<Badge>` component
//! so variants stay consistent.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-custom-badge",
    description: "Badge-shaped `<span>` drifts from the shadcn design system — use `<Badge>`.",
    remediation: "Replace `<span className=\"rounded-full bg-…\">` with `<Badge variant=\"…\">`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
