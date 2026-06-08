//! shadcn-no-manual-zindex-overlays — forbid `z-*` utilities on shadcn
//! overlay primitives (`Dialog`, `Sheet`, `Drawer`, `AlertDialog`,
//! `DropdownMenu`, `Popover`, `Tooltip`). Those components ship their
//! own z-index stack; manual overrides cause layering bugs.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-manual-zindex-overlays",
    description: "Do not set `z-*` on shadcn overlay primitives — they own the stacking order.",
    remediation: "Remove the `z-*` utility; if a layering issue remains, adjust the stacking of surrounding non-overlay elements instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
