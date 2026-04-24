//! shadcn-no-manual-zindex-overlays — forbid `z-*` utilities on shadcn
//! overlay primitives (`Dialog`, `Sheet`, `Drawer`, `AlertDialog`,
//! `DropdownMenu`, `Popover`, `Tooltip`). Those components ship their
//! own z-index stack; manual overrides cause layering bugs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-manual-zindex-overlays",
    description: "Do not set `z-*` on shadcn overlay primitives — they own the stacking order.",
    remediation: "Remove the `z-*` utility; if a layering issue remains, adjust the stacking of surrounding non-overlay elements instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
