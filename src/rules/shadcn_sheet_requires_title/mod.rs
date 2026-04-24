//! shadcn-sheet-requires-title — each `<SheetContent>` must contain
//! a `<SheetTitle>` descendant (Radix a11y requirement).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-sheet-requires-title",
    description: "`<SheetContent>` must render a `<SheetTitle>` for screen readers.",
    remediation: "Add a `<SheetTitle>` inside the sheet; use `VisuallyHidden` to keep it off-screen if needed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["shadcn", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
