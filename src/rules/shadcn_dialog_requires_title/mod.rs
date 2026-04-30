//! shadcn-dialog-requires-title — each `<DialogContent>` must contain
//! a `<DialogTitle>` descendant (Radix a11y requirement).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-dialog-requires-title",
    description: "`<DialogContent>` must render a `<DialogTitle>` for screen readers.",
    remediation: "Add a `<DialogTitle>` inside the dialog; use `VisuallyHidden` to keep it off-screen if needed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["shadcn", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
