//! tailwind-no-raw-color-utilities — flag raw palette colors
//! (`bg-white`, `text-gray-900`, `bg-blue-500`) inside a component's
//! `className`. Components should consume semantic design tokens
//! (`bg-background`, `text-foreground`, `bg-primary`) so dark mode and
//! theming work without per-element `dark:` variants.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-raw-color-utilities",
    description: "Forbid raw palette color utilities (bg-white, text-gray-900, bg-blue-500).",
    remediation: "Use semantic design tokens (bg-background, text-foreground, bg-primary, text-muted-foreground) so theming and dark mode stay centralized.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
