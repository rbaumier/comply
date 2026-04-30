//! shadcn-no-space-x-y — forbid `space-x-*` / `space-y-*` utilities in
//! JSX `className`; prefer `flex` + `gap-*`, which plays nicely with
//! shadcn layout primitives and RTL.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-space-x-y",
    description: "`space-x-*` / `space-y-*` produce brittle layouts — use flex/grid + gap-* instead.",
    remediation: "Replace `space-x-2` with `flex gap-2` and `space-y-4` with `flex flex-col gap-4`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
